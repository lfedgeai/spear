package common

import (
	"fmt"
	"os"
	"os/exec"
	"time"

	"github.com/go-vgo/robotgo"
	"github.com/lfedgeai/spear/worker/task"
	log "github.com/sirupsen/logrus"
	"github.com/twilio/twilio-go"

	twilioApi "github.com/twilio/twilio-go/rest/api/v2010"
)

type ToolId string
type ToolsetId string
type BuiltInToolCbFunc func(inv *InvocationInfo, args interface{}) (interface{}, error)

type ToolParam struct {
	Ptype       string
	Description string
	Required    bool
}

type ToolRegistry struct {
	Name        string
	Description string
	Params      map[string]ToolParam
	Cb          string
	CbBuiltIn   BuiltInToolCbFunc
}

type ToolsetRegistry struct {
	Description string
	ToolsIds    []ToolId
}

var (
	GlobalTaskTools    = map[task.TaskID]map[ToolId]ToolRegistry{}
	GlobalTaskToolsets = map[task.TaskID]map[ToolsetId]ToolsetRegistry{}
)

var (
	twilioAccountSid = os.Getenv("TWILIO_ACCOUNT_SID")
	twilioApiSecret  = os.Getenv("TWILIO_AUTH_TOKEN")
	twilioFrom       = os.Getenv("TWILIO_FROM")
)

var BuiltinTools = []ToolRegistry{
	{
		Name:        "datetime",
		Description: "Get current date and time, including timezone information",
		Params:      map[string]ToolParam{},
		Cb:          "",
		CbBuiltIn: func(inv *InvocationInfo, args interface{}) (interface{}, error) {
			return time.Now().Format(time.RFC3339), nil
		},
	},
	// {
	// 	Name:        "user_input",
	// 	Description: "Get user input",
	// 	Params: map[string]ToolParam{
	// 		"message": {
	// 			Ptype:       "string",
	// 			Description: "Message to show to user",
	// 			Required:    true,
	// 		},
	// 	},
	// 	Cb: "",
	// 	CbBuiltIn: func(inv *InvocationInfo, args interface{}) (interface{}, error) {
	// 		fmt.Printf("(NEED INPUT) %s > ", args.(map[string]interface{})["message"])
	// 		var response string
	// 		fmt.Scanln(&response)
	// 		return response, nil
	// 	},
	// },
	{
		Name:        "list_open_emails",
		Description: "List all open email drafts window",
		Params:      map[string]ToolParam{},
		Cb:          "",
		CbBuiltIn: func(inv *InvocationInfo, args interface{}) (interface{}, error) {
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
		Name: "compose_email",
		Description: `Compose an email, open a draft window with the email pre-filled. 
		NOTE: the email has to be a valid email address, you need to get it from other tools or from the user directly`,
		Params: map[string]ToolParam{
			"to": {
				Ptype:       "string",
				Description: "Email address to send email to",
				Required:    true,
			},
			"subject": {
				Ptype:       "string",
				Description: "Subject of the email",
				Required:    true,
			},
			"body": {
				Ptype:       "string",
				Description: "Body of the email",
				Required:    true,
			},
		},
		Cb: "",
		CbBuiltIn: func(inv *InvocationInfo, args interface{}) (interface{}, error) {
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
		Name: "send_email_draft_window",
		Description: `Activate the email draft window and send the email, call the tool "list_open_emails" before you continue. 
		NOTE: Before call this tool, assitant needs to stop & ask the user to say yes`,
		Params: map[string]ToolParam{
			"window_name": {
				Ptype:       "string",
				Description: "Name of the email draft window returned= from the \"list_open_emails\" tool",
				Required:    true,
			},
		},
		Cb: "",
		CbBuiltIn: func(inv *InvocationInfo, args interface{}) (interface{}, error) {
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
		Name:        "phone_call",
		Description: "Call a phone number and play a message",
		Params: map[string]ToolParam{
			"phone_number": {
				Ptype:       "string",
				Description: "Phone number to send SMS to",
				Required:    true,
			},
			"message": {
				Ptype:       "string",
				Description: "Message to send, in TwiML format",
				Required:    true,
			},
		},
		Cb: "",
		CbBuiltIn: func(inv *InvocationInfo, args interface{}) (interface{}, error) {
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
		Name:        "search_contact_email",
		Description: "Search for a person's email address in Contacts",
		Params: map[string]ToolParam{
			"name": {
				Ptype:       "string",
				Description: "Name of the contact to search for",
				Required:    true,
			},
		},
		Cb: "",
		CbBuiltIn: func(inv *InvocationInfo, args interface{}) (interface{}, error) {
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
		Name:        "mouse_right_click",
		Description: `Right click the mouse at the current location.`,
		Params:      map[string]ToolParam{},
		Cb:          "",
		CbBuiltIn: func(inv *InvocationInfo, args interface{}) (interface{}, error) {
			robotgo.Toggle("right")
			time.Sleep(300 * time.Millisecond)
			robotgo.Toggle("right", "up")
			return "Right click successful", nil
		},
	},
	{
		Name:        "mouse_left_click",
		Description: `Left click the mouse at the current location.`,
		Params:      map[string]ToolParam{},
		Cb:          "",
		CbBuiltIn: func(inv *InvocationInfo, args interface{}) (interface{}, error) {
			robotgo.Toggle("left")
			time.Sleep(300 * time.Millisecond)
			robotgo.Toggle("left", "up")
			return "Left click successful", nil
		},
	},
	{
		Name:        "open_url",
		Description: `Open a URL in the default browser`,
		Params: map[string]ToolParam{
			"url": {
				Ptype:       "string",
				Description: "URL to open",
				Required:    true,
			},
		},
		Cb: "",
		CbBuiltIn: func(inv *InvocationInfo, args interface{}) (interface{}, error) {
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
