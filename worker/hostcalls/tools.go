package hostcalls

import (
	"fmt"

	"github.com/lfedgeai/spear/pkg/rpc/payload"
	"github.com/lfedgeai/spear/pkg/utils"
	hcommon "github.com/lfedgeai/spear/worker/hostcalls/common"
	"github.com/lfedgeai/spear/worker/task"
	tsk "github.com/lfedgeai/spear/worker/task"
	log "github.com/sirupsen/logrus"
)

func NewTool(inv *hcommon.InvocationInfo, args interface{}) (interface{}, error) {
	task := *(inv.Task)
	log.Debugf("NewTool called from task [%s]", task.Name())
	taskId := task.ID()

	req := &payload.NewToolRequest{}
	if err := utils.InterfaceToType(req, args); err != nil {
		return nil, err
	}

	// check toolset exists
	if _, ok := hcommon.GlobalTaskToolsets[taskId]; !ok {
		return nil, fmt.Errorf("task with id %s does not exist", taskId)
	} else {
		if ts, ok := hcommon.GlobalTaskToolsets[taskId][hcommon.ToolsetId(req.ToolsetID)]; !ok {
			return nil, fmt.Errorf("toolset with id %d does not exist", req.ToolsetID)
		} else {
			// create tool
			newTool := hcommon.ToolRegistry{
				Name:        req.Name,
				Description: req.Description,
				Params:      make(map[string]hcommon.ToolParam),
				Cb:          req.Cb,
				CbBuiltIn:   nil,
			}
			for _, param := range req.Params {
				newTool.Params[param.Name] = hcommon.ToolParam{
					Ptype:       param.Type,
					Description: param.Description,
					Required:    param.Required,
				}
			}
			sz := len(ts.Tools)
			ts.Tools[hcommon.ToolId(sz)] = newTool
			return &payload.NewToolResponse{
				ToolsetID: int(sz),
			}, nil
		}
	}
}

func NewToolset(inv *hcommon.InvocationInfo, args interface{}) (interface{}, error) {
	task := *(inv.Task)
	log.Debugf("NewToolset called from task [%s]", task.Name())

	req := &payload.NewToolsetRequest{}
	if err := utils.InterfaceToType(req, args); err != nil {
		return nil, err
	}

	// check if task exists
	taskId := task.ID()
	if _, ok := hcommon.GlobalTaskToolsets[taskId]; !ok {
		hcommon.GlobalTaskToolsets[taskId] = map[hcommon.ToolsetId]hcommon.ToolsetRegistry{}
	}

	sz := len(hcommon.GlobalTaskToolsets[taskId])
	tsid := hcommon.ToolsetId(sz)
	if _, ok := hcommon.GlobalTaskToolsets[taskId][tsid]; ok {
		return nil, fmt.Errorf("toolset with id %d already exists", tsid)
	}
	// create toolset
	newTs := hcommon.ToolsetRegistry{
		Description:     req.Description,
		Tools:           make(map[hcommon.ToolId]hcommon.ToolRegistry),
		WorkloadToolset: nil,
		Instance:        nil,
	}

	// check if task has a workload
	if req.WorkloadName != "" {
		if wts, ok := hcommon.SearchWorkloadToolsetByName(req.WorkloadName); ok {
			newTs.WorkloadToolset = wts
			// create a new instance of the workload
			if wts.Wtype == hcommon.WorkloadTypeDocker {
				rt, err := tsk.GetTaskRuntime(tsk.TaskTypeDocker)
				if err != nil {
					return nil, fmt.Errorf("error: %v", err)
				}
				instance, err := rt.CreateTask(&tsk.TaskConfig{
					Name:  fmt.Sprintf("%s-toolset-%s", task.Name(), wts.Name),
					Cmd:   "/start",
					Args:  []string{},
					Image: wts.Name,
				})
				if err != nil {
					return nil, fmt.Errorf("error: %v", err)
				}
				err = inv.CommMgr.InstallToTask(instance)
				if err != nil {
					return nil, fmt.Errorf("error: %v", err)
				}
				instance.Start()
				newTs.Instance = instance

			} else {
				return nil, fmt.Errorf("workload type %d is not supported yet", wts.Wtype)
			}
		} else {
			return nil, fmt.Errorf("workload with id %s does not exist", req.WorkloadName)
		}
	}

	hcommon.GlobalTaskToolsets[taskId][tsid] = newTs

	return &payload.NewToolsetResponse{
		ToolsetID: int(tsid),
	}, nil
}

func ToolsetInstallBuiltins(inv *hcommon.InvocationInfo, args interface{}) (interface{}, error) {
	task := *(inv.Task)
	log.Debugf("ToolsetInstallBuiltins called from task [%s]", task.Name())

	req := &payload.ToolsetInstallBuiltinsRequest{}
	if err := utils.InterfaceToType(req, args); err != nil {
		return nil, err
	}

	// check if task exists
	taskId := task.ID()
	if _, ok := hcommon.GlobalTaskToolsets[taskId]; !ok {
		return nil, fmt.Errorf("task with id %s does not exist", taskId)
	}

	tsid := req.ToolsetID
	// add BuiltinTools to toolset
	if toolset, ok := hcommon.GlobalTaskToolsets[taskId][hcommon.ToolsetId(tsid)]; !ok {
		return nil, fmt.Errorf("toolset with id %d does not exist", tsid)
	} else {
		if toolset.Tools == nil {
			panic("toolset.Tools is nil")
		}
		idx := 0
		for _, tool := range hcommon.GetBuiltinTools() {
			// log.Infof("Adding builtin tool %s to toolset %d", tool.Name, tsid)
			toolset.Tools[hcommon.ToolId(idx)] = tool
			idx++
		}
	}

	return &payload.ToolsetInstallBuiltinsResponse{
		ToolsetID: int(tsid),
	}, nil
}

func ToolCall(inv *hcommon.InvocationInfo, args interface{}) (interface{}, error) {
	task := *(inv.Task)
	log.Debugf("ToolCall called from task [%s]", task.Name())

	req := &payload.ToolCallRequest{}
	if err := utils.InterfaceToType(req, args); err != nil {
		return nil, err
	}

	toolReg := &hcommon.ToolRegistry{}
	// check if task exists
	taskId := task.ID()
	if ts, ok := hcommon.GlobalTaskToolsets[taskId]; !ok {
		return nil, fmt.Errorf("task with id %s does not exist", taskId)
	} else {
		if toolset, ok := ts[hcommon.ToolsetId(req.ToolsetID)]; !ok {
			return nil, fmt.Errorf("toolset with id %d does not exist", req.ToolsetID)
		} else {
			if tool, ok := toolset.Tools[hcommon.ToolId(req.ToolID)]; !ok {
				return nil, fmt.Errorf("tool with id %d does not exist", req.ToolID)
			} else {
				toolReg = &tool
			}
		}
	}

	if toolReg.CbBuiltIn == nil {
		return nil, fmt.Errorf("tool with id %d is not a built-in tool", req.ToolID)
	}

	// TODO: implement tool call

	panic("not implemented")
	return nil, nil
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

func GetToolById(task task.Task, tsid hcommon.ToolsetId, tid hcommon.ToolId) (*hcommon.ToolRegistry, bool) {
	taskId := task.ID()
	if _, ok := hcommon.GlobalTaskToolsets[taskId]; !ok {
		return nil, false
	}
	if _, ok := hcommon.GlobalTaskToolsets[taskId][tsid]; !ok {
		return nil, false
	}
	if res, ok := hcommon.GlobalTaskToolsets[taskId][tsid].Tools[tid]; !ok {
		return nil, false
	} else {
		return &res, true
	}
}

func GetToolByName(task task.Task, tsid hcommon.ToolsetId, name string) (*hcommon.ToolRegistry, bool) {
	taskId := task.ID()
	if _, ok := hcommon.GlobalTaskToolsets[taskId]; !ok {
		return nil, false
	}
	if _, ok := hcommon.GlobalTaskToolsets[taskId][tsid]; !ok {
		return nil, false
	}
	for _, tool := range hcommon.GlobalTaskToolsets[taskId][tsid].Tools {
		if tool.Name == name {
			return &tool, true
		}
	}
	return nil, false
}
