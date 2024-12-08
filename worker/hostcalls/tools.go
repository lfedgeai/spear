package hostcalls

import (
	"encoding/json"
	"fmt"

	"github.com/google/uuid"

	"github.com/lfedgeai/spear/pkg/rpc/payload"
	hcommon "github.com/lfedgeai/spear/worker/hostcalls/common"
	"github.com/lfedgeai/spear/worker/task"
	log "github.com/sirupsen/logrus"
)

func NewTool(inv *hcommon.InvocationInfo, args interface{}) (interface{}, error) {
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
	if _, ok := hcommon.GlobalTaskTools[taskId]; !ok {
		hcommon.GlobalTaskTools[taskId] = make(map[hcommon.ToolId]hcommon.ToolRegistry)
	}

	tid := hcommon.ToolId(uuid.New().String())
	// create tool
	hcommon.GlobalTaskTools[taskId][tid] = hcommon.ToolRegistry{
		Name:        req.Name,
		Description: req.Description,
		Params:      make(map[string]hcommon.ToolParam),
		Cb:          req.Cb,
		CbBuiltIn:   nil,
	}

	for _, param := range req.Params {
		hcommon.GlobalTaskTools[taskId][tid].Params[param.Name] = hcommon.ToolParam{
			Ptype:       param.Type,
			Description: param.Description,
			Required:    param.Required,
		}
	}

	return &payload.NewToolResponse{
		Tid: string(tid),
	}, nil
}

func NewToolset(inv *hcommon.InvocationInfo, args interface{}) (interface{}, error) {
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
	if _, ok := hcommon.GlobalTaskToolsets[taskId]; !ok {
		hcommon.GlobalTaskToolsets[taskId] = make(map[hcommon.ToolsetId]hcommon.ToolsetRegistry)
	}

	tids := []hcommon.ToolId{}
	for _, tid := range req.ToolIds {
		// make sure tool exists
		if _, ok := hcommon.GlobalTaskTools[taskId][hcommon.ToolId(tid)]; !ok {
			return nil, fmt.Errorf("tool with id %s does not exist", tid)
		}
		tids = append(tids, hcommon.ToolId(tid))
	}

	tsid := hcommon.ToolsetId(uuid.New().String())
	// create toolset
	hcommon.GlobalTaskToolsets[taskId][tsid] = hcommon.ToolsetRegistry{
		Description: req.Description,
		ToolsIds:    tids,
	}

	return &payload.NewToolsetResponse{
		Tsid: string(tsid),
	}, nil
}

func ToolsetInstallBuiltins(inv *hcommon.InvocationInfo, args interface{}) (interface{}, error) {
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
	if _, ok := hcommon.GlobalTaskToolsets[taskId]; !ok {
		hcommon.GlobalTaskToolsets[taskId] = make(map[hcommon.ToolsetId]hcommon.ToolsetRegistry)
	}

	// install BuiltinTools to task
	tids := []hcommon.ToolId{}
	for _, tool := range hcommon.BuiltinTools {
		tid := hcommon.ToolId(uuid.New().String())
		if _, ok := hcommon.GlobalTaskTools[taskId]; !ok {
			hcommon.GlobalTaskTools[taskId] = make(map[hcommon.ToolId]hcommon.ToolRegistry)
		}
		hcommon.GlobalTaskTools[taskId][tid] = tool
		tids = append(tids, tid)
	}

	tsid := req.Tsid
	// add BuiltinTools to toolset
	if toolset, ok := hcommon.GlobalTaskToolsets[taskId][hcommon.ToolsetId(tsid)]; !ok {
		return nil, fmt.Errorf("toolset with id %s does not exist", tsid)
	} else {
		toolset.ToolsIds = append(toolset.ToolsIds, tids...)
		hcommon.GlobalTaskToolsets[taskId][hcommon.ToolsetId(tsid)] = toolset
	}

	return &payload.ToolsetInstallBuiltinsResponse{
		Tsid: tsid,
	}, nil
}

func GetToolset(task task.Task, tsid hcommon.ToolsetId) (*hcommon.ToolsetRegistry, bool) {
	taskId := task.ID()
	if _, ok := hcommon.GlobalTaskToolsets[taskId]; !ok {
		return nil, false
	}
	if _, ok := hcommon.GlobalTaskToolsets[taskId][tsid]; !ok {
		return nil, false
	}
	res := hcommon.GlobalTaskToolsets[taskId][tsid]
	return &res, true
}

func GetToolById(task task.Task, tid hcommon.ToolId) (*hcommon.ToolRegistry, bool) {
	taskId := task.ID()
	if _, ok := hcommon.GlobalTaskTools[taskId]; !ok {
		return nil, false
	}
	if _, ok := hcommon.GlobalTaskTools[taskId][tid]; !ok {
		return nil, false
	}
	res := hcommon.GlobalTaskTools[taskId][tid]
	return &res, true
}

func GetToolByName(task task.Task, name string) (*hcommon.ToolRegistry, bool) {
	taskId := task.ID()
	if _, ok := hcommon.GlobalTaskTools[taskId]; !ok {
		return nil, false
	}
	for tid, tool := range hcommon.GlobalTaskTools[taskId] {
		if tool.Name == name {
			toolreg := hcommon.GlobalTaskTools[taskId][tid]
			return &toolreg, true
		}
	}
	return nil, false
}
