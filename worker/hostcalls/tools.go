package hostcalls

import (
	"encoding/json"
	"fmt"
	"os"
	"time"

	"github.com/google/uuid"
	"github.com/twilio/twilio-go"
	twilioApi "github.com/twilio/twilio-go/rest/api/v2010"

	"github.com/lfedgeai/spear/pkg/rpc/payload"
	hostcalls "github.com/lfedgeai/spear/worker/hostcalls/common"
	"github.com/lfedgeai/spear/worker/task"
	log "github.com/sirupsen/logrus"
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
			description: "Get current date and time",
			params:      map[string]ToolParam{},
			cb:          "",
			cbBuiltIn: func(inv *hostcalls.InvocationInfo, args interface{}) (interface{}, error) {
				return time.Now().Format(time.RFC3339), nil
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
