package tools

import (
	"os/exec"

	hccommon "github.com/lfedgeai/spear/worker/hostcalls/common"
	log "github.com/sirupsen/logrus"
)

var emailTools = []hccommon.ToolRegistry{
	{
		Name:        "list_open_emails",
		Description: "List all open email drafts window",
		Params:      map[string]hccommon.ToolParam{},
		Cb:          "",
		CbBuiltIn:   listOpenEmails,
	},
	{
		Name: "compose_email",
		Description: `Compose an email, open a draft window with the email pre-filled. 
		NOTE: the email has to be a valid email address, you need to get it from other tools or from the user directly`,
		Params: map[string]hccommon.ToolParam{
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
		Cb:        "",
		CbBuiltIn: composeEmail,
	},
	{
		Name: "send_email_draft_window",
		Description: `Activate the email draft window and send the email. 
		NOTE: 1. Call the tool "list_open_emails" to list available email windows before calling this function. 
		2. Before call this tool to actually send the email, assitant needs to stop & ask the user to say yes`,
		Params: map[string]hccommon.ToolParam{
			"window_name": {
				Ptype:       "string",
				Description: "Name of the email draft window returned= from the \"list_open_emails\" tool",
				Required:    true,
			},
		},
		Cb:        "",
		CbBuiltIn: sendEmailDraftWindow,
	},
}

func listOpenEmails(inv *hccommon.InvocationInfo, args interface{}) (interface{}, error) {
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
}

func composeEmail(inv *hccommon.InvocationInfo, args interface{}) (interface{}, error) {
	// use apple script to open mail app and compose email
	params := args.(map[string]interface{})
	subject, ok := params["subject"].(string)
	if !ok {
		subject = ""
	}
	to, ok := params["to"].(string)
	if !ok {
		to = ""
	}
	body, ok := params["body"].(string)
	if !ok {
		body = ""
	}
	script := `tell application "Microsoft Outlook"
		activate
		set newMessage to make new outgoing message with properties {subject:"` + subject + `", content:"` + body + `"}
		make new recipient at newMessage with properties {email address:{address:"` + to + `"}}
		open newMessage
		-- get the name of the window
		set windowName to name of window 1
		do shell script "echo the name of the new draft window is: " & quoted form of windowName
	end tell`
	out, err := exec.Command("osascript", "-e", script).Output()
	if err != nil {
		return nil, err
	}
	return string(out), nil
}

func sendEmailDraftWindow(inv *hccommon.InvocationInfo, args interface{}) (interface{}, error) {
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
				do shell script "echo 'Failed to find the window with title starting with \"" & targetPrefix & "\".' Do you need to call the \"list_open_emails\" tool again?"
			end if
		end tell`
	out, err := exec.Command("osascript", "-e", script).Output()
	if err != nil {
		return nil, err
	}
	// return the output of the script
	return string(out), nil
}

func init() {
	for _, tool := range emailTools {
		hccommon.RegisterBuiltinTool(tool)
	}
}
