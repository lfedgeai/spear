package tools

import (
	"os/exec"

	hccommon "github.com/lfedgeai/spear/spearlet/hostcalls/common"
	log "github.com/sirupsen/logrus"
)

var contactTools = []hccommon.ToolRegistry{
	{
		ToolType:    hccommon.ToolType_Builtin,
		Name:        "search_contact_email",
		Id:          hccommon.BuiltinToolID_SearchContactEmail,
		Description: "Search for a person's email address in Contacts",
		Params: map[string]hccommon.ToolParam{
			"name": {
				Ptype:       "string",
				Description: "Name of the contact to search for",
				Required:    true,
			},
		},
		CbBuiltIn: func(inv *hccommon.InvocationInfo, args interface{}) (interface{}, error) {
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
}

func init() {
	for _, tool := range contactTools {
		hccommon.RegisterBuiltinTool(tool)
	}
}
