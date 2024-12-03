package hostcalls

import (
	"encoding/json"
	"fmt"
	"os"
	"os/exec"
	"time"

	"github.com/google/uuid"
	"github.com/twilio/twilio-go"
	twilioApi "github.com/twilio/twilio-go/rest/api/v2010"

	"github.com/lfedgeai/spear/pkg/rpc/payload"
	hostcalls "github.com/lfedgeai/spear/worker/hostcalls/common"
	"github.com/lfedgeai/spear/worker/task"
	log "github.com/sirupsen/logrus"

	"github.com/go-vgo/robotgo"
)

type ToolId string
type ToolsetId string
type BuiltInToolCbFunc func(inv *hostcalls.InvocationInfo, args interface{}) (interface{}, error)

type ToolParam struct {
	ptype       string
	description string
	required    bool
}

type ToolRegistry struct {
	name        string
	description string
	params      map[string]ToolParam
	cb          string
	cbBuiltIn   BuiltInToolCbFunc
}

type ToolsetRegistry struct {
	description string
	toolsIds    []ToolId
}

var (
	globalTaskTools    = map[task.TaskID]map[ToolId]ToolRegistry{}
	globalTaskToolsets = map[task.TaskID]map[ToolsetId]ToolsetRegistry{}

	twilioAccountSid = os.Getenv("TWILIO_ACCOUNT_SID")
	twilioApiSecret  = os.Getenv("TWILIO_AUTH_TOKEN")
	twilioFrom       = os.Getenv("TWILIO_FROM")

	builtinTools = []ToolRegistry{
		{
			name:        "datetime",
			description: "Get current date and time, including timezone information",
			params:      map[string]ToolParam{},
			cb:          "",
			cbBuiltIn: func(inv *hostcalls.InvocationInfo, args interface{}) (interface{}, error) {
				return time.Now().Format(time.RFC3339), nil
			},
		},
		{
			name:        "user_input",
			description: "Get user input",
			params: map[string]ToolParam{
				"message": {
					ptype:       "string",
					description: "Message to show to user",
					required:    true,
				},
			},
			cb: "",
			cbBuiltIn: func(inv *hostcalls.InvocationInfo, args interface{}) (interface{}, error) {
				fmt.Println(args.(map[string]interface{})["message"])
				var response string
				fmt.Scanln(&response)
				return response, nil
			},
		},
		{
			name:        "list_open_emails",
			description: "List all open email drafts window",
			params:      map[string]ToolParam{},
			cb:          "",
			cbBuiltIn: func(inv *hostcalls.InvocationInfo, args interface{}) (interface{}, error) {
				// use apple script to list all open email drafts window
				script := `tell application "Microsoft Outlook"
					activate
					set windowList to every window
					set windowDetails to {}
					
					repeat with win in windowList
						set end of windowDetails to name of win -- Collect the name (title) of each window
					end repeat
				end tell

				if windowDetails is {} then
					display dialog "No open windows found in Mail."
				else
					set AppleScript's text item delimiters to linefeed
					set windowInfo to windowDetails as text
					set AppleScript's text item delimiters to "" -- Reset delimiters
					do shell script "echo " & quoted form of ("Open Windows in Mail:" & return & windowInfo)
				end if`
				out, err := exec.Command("osascript", "-e", script).Output()
				if err != nil {
					return nil, err
				}
				return string(out), nil
			},
		},
		{
			name: "compose_email",
			description: `Compose an email, open a draft window with the email pre-filled. 
			NOTE: the email has to be a valid email address, you need to get it from other tools or from the user directly`,
			params: map[string]ToolParam{
				"to": {
					ptype:       "string",
					description: "Email address to send email to",
					required:    true,
				},
				"subject": {
					ptype:       "string",
					description: "Subject of the email",
					required:    true,
				},
				"body": {
					ptype:       "string",
					description: "Body of the email",
					required:    true,
				},
			},
			cb: "",
			cbBuiltIn: func(inv *hostcalls.InvocationInfo, args interface{}) (interface{}, error) {
				// use apple script to open mail app and compose email
				script := `tell application "Microsoft Outlook"
					activate
					set newMessage to make new outgoing message with properties {subject:"` + args.(map[string]interface{})["subject"].(string) + `", content:"` + args.(map[string]interface{})["body"].(string) + `"}
					make new recipient at newMessage with properties {email address:{address:"` + args.(map[string]interface{})["to"].(string) + `"}}
					open newMessage
				end tell`
				_, err := exec.Command("osascript", "-e", script).Output()
				if err != nil {
					return nil, err
				}
				return fmt.Sprintf("Email to %s composed successfully", args.(map[string]interface{})["to"].(string)), nil
			},
		},
		{
			name: "send_email_draft_window",
			description: `Activate the email draft window and send the email, call the tool "list_open_emails" before you continue. 
			NOTE: Before call this tool, assitant needs to stop & ask the user to say yes`,
			params: map[string]ToolParam{
				"window_name": {
					ptype:       "string",
					description: "Name of the email draft window returned= from the \"list_open_emails\" tool",
					required:    true,
				},
			},
			cb: "",
			cbBuiltIn: func(inv *hostcalls.InvocationInfo, args interface{}) (interface{}, error) {
				// use apple script to send email
				log.Infof("Sending email with window name %s", args.(map[string]interface{})["window_name"].(string))
				script := `set targetPrefix to "` + args.(map[string]interface{})["window_name"].(string) + `"
					tell application "Microsoft Outlook"
						activate
						set windowList to every window
						set targetWindow to missing value
						
						-- Find the window with the specified prefix
						repeat with win in windowList
							set winName to name of win
							if winName starts with targetPrefix then
								set targetWindow to win
								exit repeat -- Stop searching after finding the first match
							end if
						end repeat
						
						if targetWindow is not missing value then							
							-- Attempt to send the email in the target window
							try
								tell targetWindow
									activate
									set index to 1
									tell application "System Events"
										key code 36 using command down
									end tell
								end tell
								do shell script "echo 'Email sent successfully from window: " & name of targetWindow & "'"
							on error errMsg
								do shell script "echo 'Failed to send the email: " & errMsg & "'"
							end try
						else
							do shell script "echo 'No window found with title starting with \"" & targetPrefix & "\".'"
						end if
					end tell`
				_, err := exec.Command("osascript", "-e", script).Output()
				if err != nil {
					return nil, err
				}
				return fmt.Sprintf("Email with window name %s sent successfully", args.(map[string]interface{})["window_name"].(string)), nil
			},
		},
		{
			name:        "phone_call",
			description: "Call a phone number and play a message",
			params: map[string]ToolParam{
				"phone_number": {
					ptype:       "string",
					description: "Phone number to send SMS to",
					required:    true,
				},
				"message": {
					ptype:       "string",
					description: "Message to send, in TwiML format",
					required:    true,
				},
			},
			cb: "",
			cbBuiltIn: func(inv *hostcalls.InvocationInfo, args interface{}) (interface{}, error) {
				if twilioAccountSid == "" || twilioApiSecret == "" {
					return nil, fmt.Errorf("twilio credentials not set")
				}
				client := twilio.NewRestClientWithParams(twilio.ClientParams{
					Username: twilioAccountSid,
					Password: twilioApiSecret,
				})
				params := &twilioApi.CreateCallParams{}
				params.SetTo(args.(map[string]interface{})["phone_number"].(string))
				params.SetFrom(twilioFrom)
				params.SetTwiml(args.(map[string]interface{})["message"].(string))
				_, err := client.Api.CreateCall(params)
				if err != nil {
					return nil, err
				}
				return fmt.Sprintf("Call to %s successful", args.(map[string]interface{})["phone_number"].(string)), nil
			},
		},
		{
			name:        "search_contact_email",
			description: "Search for a person's email address in Contacts",
			params: map[string]ToolParam{
				"name": {
					ptype:       "string",
					description: "Name of the contact to search for",
					required:    true,
				},
			},
			cb: "",
			cbBuiltIn: func(inv *hostcalls.InvocationInfo, args interface{}) (interface{}, error) {
				// use apple script to search for contact
				log.Infof("Searching for contact with name %s", args.(map[string]interface{})["name"].(string))
				script := `set personName to "` + args.(map[string]interface{})["name"].(string) + `"
					set foundEmails to {}

					tell application "Contacts"
						activate
						set peopleList to (every person whose name contains personName)
						repeat with p in peopleList
							set emailsList to emails of p
							repeat with e in emailsList
								set end of foundEmails to value of e
							end repeat
						end repeat
					end tell


					set AppleScript's text item delimiters to ", "
					if foundEmails is {} then
						do shell script "echo " & "No email addresses found for " & personName
					else
						do shell script "echo " & "Email addresses found: " & return & (foundEmails as text)
					end if`
				out, err := exec.Command("osascript", "-e", script).Output()
				if err != nil {
					return nil, err
				}
				return string(out), nil
			},
		},
		{
			name:        "mouse_right_click",
			description: `Right click the mouse at the current location.`,
			params:      map[string]ToolParam{},
			cb:          "",
			cbBuiltIn: func(inv *hostcalls.InvocationInfo, args interface{}) (interface{}, error) {
				robotgo.Toggle("right")
				time.Sleep(300 * time.Millisecond)
				robotgo.Toggle("right", "up")
				return "Right click successful", nil
			},
		},
		{
			name:        "mouse_left_click",
			description: `Left click the mouse at the current location.`,
			params:      map[string]ToolParam{},
			cb:          "",
			cbBuiltIn: func(inv *hostcalls.InvocationInfo, args interface{}) (interface{}, error) {
				robotgo.Toggle("left")
				time.Sleep(300 * time.Millisecond)
				robotgo.Toggle("left", "up")
				return "Left click successful", nil
			},
		},
		{
			name:        "open_url",
			description: `Open a URL in the default browser`,
			params: map[string]ToolParam{
				"url": {
					ptype:       "string",
					description: "URL to open",
					required:    true,
				},
			},
			cb: "",
			cbBuiltIn: func(inv *hostcalls.InvocationInfo, args interface{}) (interface{}, error) {
				// use apple script to open URL
				script := `set targetURL to "` + args.(map[string]interface{})["url"].(string) + `"
					tell application "Edge"
						activate
						open location targetURL
					end tell`
				_, err := exec.Command("osascript", "-e", script).Output()
				if err != nil {
					return nil, err
				}
				return fmt.Sprintf("URL %s opened successfully", args.(map[string]interface{})["url"].(string)), nil
			},
		},
	}
)

func NewTool(inv *hostcalls.InvocationInfo, args interface{}) (interface{}, error) {
	task := *(inv.Task)
	log.Debugf("NewTool called from task [%s]", task.Name())
	taskId := task.ID()

	// args is a NewToolRequest
	jsonBytes, err := json.Marshal(args)
	if err != nil {
		return nil, err
	}

	req := &payload.NewToolRequest{}
	err = req.Unmarshal(jsonBytes)
	if err != nil {
		return nil, err
	}

	// check if task exists
	if _, ok := globalTaskTools[taskId]; !ok {
		globalTaskTools[taskId] = make(map[ToolId]ToolRegistry)
	}

	tid := ToolId(uuid.New().String())
	// create tool
	globalTaskTools[taskId][tid] = ToolRegistry{
		name:        req.Name,
		description: req.Description,
		params:      make(map[string]ToolParam),
		cb:          req.Cb,
		cbBuiltIn:   nil,
	}

	for _, param := range req.Params {
		globalTaskTools[taskId][tid].params[param.Name] = ToolParam{
			ptype:       param.Type,
			description: param.Description,
			required:    param.Required,
		}
	}

	return &payload.NewToolResponse{
		Tid: string(tid),
	}, nil
}

func NewToolset(inv *hostcalls.InvocationInfo, args interface{}) (interface{}, error) {
	task := *(inv.Task)
	log.Debugf("NewToolset called from task [%s]", task.Name())

	// args is a NewToolsetRequest
	jsonBytes, err := json.Marshal(args)
	if err != nil {
		return nil, err
	}

	req := &payload.NewToolsetRequest{}
	err = req.Unmarshal(jsonBytes)
	if err != nil {
		return nil, err
	}

	// check if task exists
	taskId := task.ID()
	if _, ok := globalTaskToolsets[taskId]; !ok {
		globalTaskToolsets[taskId] = make(map[ToolsetId]ToolsetRegistry)
	}

	tids := []ToolId{}
	for _, tid := range req.ToolIds {
		// make sure tool exists
		if _, ok := globalTaskTools[taskId][ToolId(tid)]; !ok {
			return nil, fmt.Errorf("tool with id %s does not exist", tid)
		}
		tids = append(tids, ToolId(tid))
	}

	tsid := ToolsetId(uuid.New().String())
	// create toolset
	globalTaskToolsets[taskId][tsid] = ToolsetRegistry{
		description: req.Description,
		toolsIds:    tids,
	}

	return &payload.NewToolsetResponse{
		Tsid: string(tsid),
	}, nil
}

func ToolsetInstallBuiltins(inv *hostcalls.InvocationInfo, args interface{}) (interface{}, error) {
	task := *(inv.Task)
	log.Debugf("ToolsetInstallBuiltins called from task [%s]", task.Name())

	// args is a ToolsetInstallBuiltinsRequest
	jsonBytes, err := json.Marshal(args)
	if err != nil {
		return nil, err
	}

	req := &payload.ToolsetInstallBuiltinsRequest{}
	err = req.Unmarshal(jsonBytes)
	if err != nil {
		return nil, err
	}

	// check if task exists
	taskId := task.ID()
	if _, ok := globalTaskToolsets[taskId]; !ok {
		globalTaskToolsets[taskId] = make(map[ToolsetId]ToolsetRegistry)
	}

	// install builtinTools to task
	tids := []ToolId{}
	for _, tool := range builtinTools {
		tid := ToolId(uuid.New().String())
		if _, ok := globalTaskTools[taskId]; !ok {
			globalTaskTools[taskId] = make(map[ToolId]ToolRegistry)
		}
		globalTaskTools[taskId][tid] = tool
		tids = append(tids, tid)
	}

	tsid := req.Tsid
	// add builtinTools to toolset
	if toolset, ok := globalTaskToolsets[taskId][ToolsetId(tsid)]; !ok {
		return nil, fmt.Errorf("toolset with id %s does not exist", tsid)
	} else {
		toolset.toolsIds = append(toolset.toolsIds, tids...)
		globalTaskToolsets[taskId][ToolsetId(tsid)] = toolset
	}

	return &payload.ToolsetInstallBuiltinsResponse{
		Tsid: tsid,
	}, nil
}

func GetToolset(task task.Task, tsid ToolsetId) (*ToolsetRegistry, bool) {
	taskId := task.ID()
	if _, ok := globalTaskToolsets[taskId]; !ok {
		return nil, false
	}
	if _, ok := globalTaskToolsets[taskId][tsid]; !ok {
		return nil, false
	}
	res := globalTaskToolsets[taskId][tsid]
	return &res, true
}

func GetToolById(task task.Task, tid ToolId) (*ToolRegistry, bool) {
	taskId := task.ID()
	if _, ok := globalTaskTools[taskId]; !ok {
		return nil, false
	}
	if _, ok := globalTaskTools[taskId][tid]; !ok {
		return nil, false
	}
	res := globalTaskTools[taskId][tid]
	return &res, true
}

func GetToolByName(task task.Task, name string) (*ToolRegistry, bool) {
	taskId := task.ID()
	if _, ok := globalTaskTools[taskId]; !ok {
		return nil, false
	}
	for tid, tool := range globalTaskTools[taskId] {
		if tool.name == name {
			toolreg := globalTaskTools[taskId][tid]
			return &toolreg, true
		}
	}
	return nil, false
}
