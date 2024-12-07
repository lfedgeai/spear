package task

import (
	"fmt"

	log "github.com/sirupsen/logrus"
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

	supportedTaskTypes = []TaskType{}
)

// message type []bytes
type Message []byte

type TaskID string

type TaskVar int

const (
	TVOpenAIBaseURL TaskVar = iota
	TVOpenAIAPIKey
)

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
	// set task variable
	SetVar(key TaskVar, value interface{})
	// get task variable
	GetVar(key TaskVar) (interface{}, bool)
}

// interface for taskruntime
type TaskRuntime interface {
	// create task
	CreateTask(cfg *TaskConfig) (Task, error)
	Start() error
	Stop() error
}

// implement TaskRuntimeDylib
type DylibTaskRuntime struct {
}

func (d *DylibTaskRuntime) CreateTask(cfg *TaskConfig) (Task, error) {
	return nil, fmt.Errorf("not implemented")
}

func (d *DylibTaskRuntime) Start() error {
	return nil
}

func (d *DylibTaskRuntime) Stop() error {
	return nil
}

// implement TaskRuntimeWasm
type WasmTaskRuntime struct {
}

func (w *WasmTaskRuntime) CreateTask(cfg *TaskConfig) (Task, error) {
	return nil, fmt.Errorf("not implemented")
}

func (w *WasmTaskRuntime) Start() error {
	return nil
}

func (w *WasmTaskRuntime) Stop() error {
	return nil
}

type TaskRuntimeConfig struct {
	Debug         bool
	Cleanup       bool
	StartServices bool
}

// initialize task runtimes
func InitTaskRuntimes(cfg *TaskRuntimeConfig) {
	if len(supportedTaskTypes) == 0 {
		panic("no supported task types")
	}
	for _, taskType := range supportedTaskTypes {
		switch taskType {
		case TaskTypeDocker:
			rt, err := NewDockerTaskRuntime(cfg)
			if err != nil {
				panic(err)
			}
			globalTaskRuntimes[TaskTypeDocker] = rt
		case TaskTypeProcess:
			globalTaskRuntimes[TaskTypeProcess] = NewProcessTaskRuntime()
		case TaskTypeDylib:
			globalTaskRuntimes[TaskTypeDylib] = &DylibTaskRuntime{}
		case TaskTypeWasm:
			globalTaskRuntimes[TaskTypeWasm] = &WasmTaskRuntime{}
		default:
			panic("invalid task type")
		}
	}
}

func StopTaskRuntimes() {
	for rtName, rt := range globalTaskRuntimes {
		log.Debugf("Stopping task runtime: %v", rtName)
		rt.Stop()
	}
}

// register task runtime
func RegisterSupportedTaskType(taskType TaskType) {
	for _, t := range supportedTaskTypes {
		if t == taskType {
			log.Warnf("task runtime already registered: %v", taskType)
			return
		}
	}
	supportedTaskTypes = append(supportedTaskTypes, taskType)
}

// unregister task runtime
func UnregisterSupportedTaskType(taskType TaskType) {
	for i, t := range supportedTaskTypes {
		if t == taskType {
			supportedTaskTypes = append(supportedTaskTypes[:i], supportedTaskTypes[i+1:]...)
			return
		}
	}
}

// factory method for TaskRuntime
func GetTaskRuntime(taskType TaskType) (TaskRuntime, error) {
	if rt, ok := globalTaskRuntimes[taskType]; ok {
		return rt, nil
	}
	return nil, fmt.Errorf("task runtime not found")
}
