package tools

import (
	"fmt"
	"os"

	hccommon "github.com/lfedgeai/spear/spearlet/hostcalls/common"
	"github.com/twilio/twilio-go"

	twilioApi "github.com/twilio/twilio-go/rest/api/v2010"
)

var (
	twilioAccountSid = os.Getenv("TWILIO_ACCOUNT_SID")
	twilioApiSecret  = os.Getenv("TWILIO_AUTH_TOKEN")
	twilioFrom       = os.Getenv("TWILIO_FROM")
)

var phoneTools = []hccommon.ToolRegistry{
	{
		ToolType:    hccommon.ToolType_Builtin,
		Name:        "phone_call",
		Id:          hccommon.BuiltinToolID_PhoneCall,
		Description: "Call a phone number and play a message",
		Params: map[string]hccommon.ToolParam{
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
		CbBuiltIn: func(inv *hccommon.InvocationInfo, args interface{}) (interface{}, error) {
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
}

func init() {
	for _, tool := range phoneTools {
		hccommon.RegisterBuiltinTool(tool)
	}
}
