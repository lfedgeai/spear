package task

import (
	"fmt"
)

type TaskConfig struct {
	// task name
	Name  string
	Image string
	Cmd   string
	Args  []string
}

// task type enum
type TaskType int

const (
	TaskTypeUnknown TaskType = iota
	TaskTypeDocker           // 1
	TaskTypeProcess          // 2
	TaskTypeDylib            // 3
	TaskTypeWasm             // 4
)

// task status enum
type TaskStatus int

const (
	TaskStatusRunning TaskStatus = iota
	TaskStatusInit
	TaskStatusStopped
)

const (
	maxDataSize = 4096 * 1024
)

// global task runtimes
var (
	globalTaskRuntimes = make(map[TaskType]TaskRuntime)
)

// message type []bytes
type Message []byte

type TaskID string

type Task interface {
	ID() TaskID
	// start task
	Start() error
	// stop task
	Stop() error
	// get task name
	Name() string
	// get task status
	Status() TaskStatus
	// get task result
	GetResult() *error
	// get communication channel
	CommChannels() (chan Message, chan Message, error)
	// wait for task to finish
	Wait() (int, error)
	// next request id
	NextRequestID() uint64
}

// interface for taskruntime
type TaskRuntime interface {
	// create task
	CreateTask(cfg *TaskConfig) (Task, error)
}

// implement TaskRuntimeDylib
type DylibTaskRuntime struct {
}

func (d *DylibTaskRuntime) CreateTask(cfg *TaskConfig) (Task, error) {
	return nil, fmt.Errorf("not implemented")
}

// implement TaskRuntimeWasm
type WasmTaskRuntime struct {
}

func (w *WasmTaskRuntime) CreateTask(cfg *TaskConfig) (Task, error) {
	return nil, fmt.Errorf("not implemented")
}

type TaskRuntimeConfig struct {
	Debug bool
}

// factory method for TaskRuntime
func GetTaskRuntime(taskType TaskType, cfg *TaskRuntimeConfig) (TaskRuntime, error) {
	if rt, ok := globalTaskRuntimes[taskType]; ok {
		return rt, nil
	}

	var rt TaskRuntime
	var err error
	switch taskType {
	case TaskTypeDocker:
		rt, err = NewDockerTaskRuntime(cfg)
		if err != nil {
			return nil, err
		}
	case TaskTypeProcess:
		rt = NewProcessTaskRuntime()
	case TaskTypeDylib:
		rt = &DylibTaskRuntime{}
	case TaskTypeWasm:
		rt = &WasmTaskRuntime{}
	default:
		return nil, fmt.Errorf("invalid task type")
	}

	globalTaskRuntimes[taskType] = rt
	return rt, nil
}
