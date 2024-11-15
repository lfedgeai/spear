package hostcalls

import (
	"encoding/json"
	"fmt"

	"github.com/google/uuid"

	"github.com/lfedgeai/spear/pkg/rpc/payload"
	hostcalls "github.com/lfedgeai/spear/worker/hostcalls/common"
	"github.com/lfedgeai/spear/worker/task"
	log "github.com/sirupsen/logrus"
)

type ToolId string
type ToolsetId string

type ToolParam struct {
	ptype       string
	description string
	required    bool
	cb          string
}

type ToolRegistry struct {
	name        string
	description string
	params      map[string]ToolParam
}

type ToolsetRegistry struct {
	description string
	toolsIds    []ToolId
}

var (
	globalTaskTools    = map[task.TaskID]map[ToolId]ToolRegistry{}
	globalTaskToolsets = map[task.TaskID]map[ToolsetId]ToolsetRegistry{}
)

func NewTool(caller *hostcalls.Caller, args interface{}) (interface{}, error) {
	task := *(caller.Task)
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
	}

	for _, param := range req.Params {
		globalTaskTools[taskId][tid].params[param.Name] = ToolParam{
			ptype:       param.Type,
			description: param.Description,
			required:    param.Required,
			cb:          param.Cb,
		}
	}

	return &payload.NewToolResponse{
		Tid: string(tid),
	}, nil
}

func NewToolset(caller *hostcalls.Caller, args interface{}) (interface{}, error) {
	task := *(caller.Task)
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

func GetToolset(task task.Task, tsid ToolsetId) (ToolsetRegistry, error) {
	taskId := task.ID()
	if _, ok := globalTaskToolsets[taskId]; !ok {
		return ToolsetRegistry{}, fmt.Errorf("task has no toolsets")
	}
	if _, ok := globalTaskToolsets[taskId][tsid]; !ok {
		return ToolsetRegistry{}, fmt.Errorf("toolset with id %s does not exist", tsid)
	}
	return globalTaskToolsets[taskId][tsid], nil
}

func GetTool(task task.Task, tid ToolId) (ToolRegistry, error) {
	taskId := task.ID()
	if _, ok := globalTaskTools[taskId]; !ok {
		return ToolRegistry{}, fmt.Errorf("task has no tools")
	}
	if _, ok := globalTaskTools[taskId][tid]; !ok {
		return ToolRegistry{}, fmt.Errorf("tool with id %s does not exist", tid)
	}
	return globalTaskTools[taskId][tid], nil
}
