package task

import (
	"fmt"
)

type TaskConfig struct {
	// task name
	Name string
	Cmd  string
	Args []string
}

// task type enum
type TaskType int

const (
	TaskTypeDocker TaskType = iota
	TaskTypeProcess
	TaskTypeDylib
	TaskTypeWasm
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

// message type []bytes
type Message []byte

type TaskID int64

type Task interface {
	ID() TaskID
	// start task
	Start()
	// stop task
	Stop()
	// get task name
	Name() string
	// get task status
	Status() TaskStatus
	// get task result
	GetResult() *error
	// get communication channel
	CommChannels() (chan Message, chan Message, error)
	// wait for task to finish
	Wait()
}

// interface for taskruntime
type TaskRuntime interface {
	// create task
	CreateTask(cfg *TaskConfig) (Task, error)
}

// implement TaskRuntimeDocker
type DockerTaskRuntime struct {
}

func (d *DockerTaskRuntime) CreateTask(cfg *TaskConfig) (Task, error) {
	return nil, fmt.Errorf("not implemented")
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

// factory method for TaskRuntime
func NewTaskRuntime(taskType TaskType) (TaskRuntime, error) {
	switch taskType {
	case TaskTypeDocker:
		return &DockerTaskRuntime{}, nil
	case TaskTypeProcess:
		return &ProcessTaskRuntime{}, nil
	case TaskTypeDylib:
		return &DylibTaskRuntime{}, nil
	case TaskTypeWasm:
		return &WasmTaskRuntime{}, nil
	default:
		return nil, fmt.Errorf("invalid task type")
	}
}
