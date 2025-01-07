package spearlet

import (
	"context"
	"encoding/json"
	"fmt"
	"io"
	"math/rand"
	"net/http"
	"os"
	"path/filepath"
	"strconv"
	"time"

	flatbuffers "github.com/google/flatbuffers/go"
	log "github.com/sirupsen/logrus"

	"github.com/lfedgeai/spear/pkg/common"
	"github.com/lfedgeai/spear/pkg/spear/proto/custom"
	"github.com/lfedgeai/spear/pkg/spear/proto/transport"
	hc "github.com/lfedgeai/spear/spearlet/hostcalls"
	hostcalls "github.com/lfedgeai/spear/spearlet/hostcalls/common"
	"github.com/lfedgeai/spear/spearlet/task"
	_ "github.com/lfedgeai/spear/spearlet/tools"
)

var (
	logLevel = log.InfoLevel
)

type SpearletConfig struct {
	Addr string
	Port string

	// Search Path
	SearchPath []string

	// Debug
	Debug          bool
	LocalExecution bool

	SpearAddr string
}

type Spearlet struct {
	cfg *SpearletConfig
	mux *http.ServeMux
	srv *http.Server

	SearchPaths []string
	hc          *hostcalls.HostCalls
	commMgr     *hostcalls.CommunicationManager

	spearAddr string
}

type TaskMetaData struct {
	Id        int64
	Type      task.TaskType
	ImageName string
	ExecName  string
	Name      string
}

var (
	tmpMetaData = map[int]TaskMetaData{
		3: {
			Id:        3,
			Type:      task.TaskTypeDocker,
			ImageName: "gen_image:latest",
			Name:      "gen_image",
		},
		4: {
			Id:        4,
			Type:      task.TaskTypeDocker,
			ImageName: "pychat:latest",
			Name:      "pychat",
		},
		5: {
			Id:        5,
			Type:      task.TaskTypeDocker,
			ImageName: "pytools:latest",
			Name:      "pytools",
		},
		6: {
			Id:        6,
			Type:      task.TaskTypeDocker,
			ImageName: "pyconversation:latest",
			Name:      "pyconversation",
		},
		7: {
			Id:        7,
			Type:      task.TaskTypeDocker,
			ImageName: "pydummy:latest",
			Name:      "pydummy",
		},
		8: {
			Id:        8,
			Type:      task.TaskTypeDocker,
			ImageName: "pytest-functionality:latest",
			Name:      "pytest-functionality",
		},
		11: {
			Id:       11,
			Type:     task.TaskTypeProcess,
			ExecName: "pytest-functionality.py",
			Name:     "pytest-functionality-proc",
		},
	}
)

// NewServeSpearletConfig creates a new SpearletConfig
func NewServeSpearletConfig(addr, port string, spath []string, debug bool,
	spearAddr string) *SpearletConfig {
	return &SpearletConfig{
		Addr:           addr,
		Port:           port,
		SearchPath:     spath,
		Debug:          debug,
		LocalExecution: false,
		SpearAddr:      spearAddr,
	}
}

func NewExecSpearletConfig(debug bool, spearAddr string, spath []string) *SpearletConfig {
	return &SpearletConfig{
		Addr:           "",
		Port:           "",
		SearchPath:     spath,
		Debug:          debug,
		LocalExecution: true,
		SpearAddr:      spearAddr,
	}
}

func NewSpearlet(cfg *SpearletConfig) *Spearlet {
	w := &Spearlet{
		cfg:       cfg,
		mux:       http.NewServeMux(),
		hc:        nil,
		commMgr:   hostcalls.NewCommunicationManager(),
		spearAddr: cfg.SpearAddr,
	}
	hc := hostcalls.NewHostCalls(w.commMgr)
	w.hc = hc
	return w
}

func (w *Spearlet) Initialize() {
	w.addRoutes()
	w.addHostCalls()
	w.initializeRuntimes()
	go w.hc.Run()
}

func (w *Spearlet) addHostCalls() {
	for _, hc := range hc.Hostcalls {
		w.hc.RegisterHostCall(hc)
	}
}

func (w *Spearlet) initializeRuntimes() {
	cfg := &task.TaskRuntimeConfig{
		Debug:         w.cfg.Debug,
		Cleanup:       true,
		StartServices: true,
	}
	task.RegisterSupportedTaskType(task.TaskTypeDocker)
	task.RegisterSupportedTaskType(task.TaskTypeProcess)
	task.InitTaskRuntimes(cfg)
}

func funcAsync(req *http.Request) (bool, error) {
	// get request headers
	headers := req.Header
	// get the async from the headers
	async := headers.Get(HeaderFuncAsync)
	if async == "" {
		return false, nil
	}

	// convert async to bool
	b, err := strconv.ParseBool(async)
	if err != nil {
		return false, fmt.Errorf("error parsing %s header: %v", HeaderFuncAsync, err)
	}

	return b, nil
}

func funcId(req *http.Request) (int64, error) {
	// get request headers
	headers := req.Header
	// get the id from the headers
	id := headers.Get(HeaderFuncId)
	if id == "" {
		return -1, fmt.Errorf("missing %s header", HeaderFuncId)
	}

	// convert id to int64
	i, err := strconv.ParseInt(id, 10, 64)
	if err != nil {
		return -1, fmt.Errorf("error parsing %s header: %v", HeaderFuncId, err)
	}

	return i, nil
}

func funcType(req *http.Request) (task.TaskType, error) {
	// get request headers
	headers := req.Header
	// get the runtime from the headers
	runtime := headers.Get(HeaderFuncType)
	if runtime == "" {
		return task.TaskTypeUnknown,
			fmt.Errorf("missing %s header", HeaderFuncType)
	}

	// convert runtime to int
	i, err := strconv.Atoi(runtime)
	if err != nil {
		return task.TaskTypeUnknown,
			fmt.Errorf("error parsing %s header: %v", HeaderFuncType, err)
	}

	switch i {
	case int(task.TaskTypeDocker):
		return task.TaskTypeDocker, nil
	case int(task.TaskTypeProcess):
		return task.TaskTypeProcess, nil
	case int(task.TaskTypeDylib):
		return task.TaskTypeDylib, nil
	case int(task.TaskTypeWasm):
		return task.TaskTypeWasm, nil
	default:
		return task.TaskTypeUnknown,
			fmt.Errorf("invalid %s header: %s", HeaderFuncType, runtime)
	}
}

func (w *Spearlet) LookupTaskId(name string) (int64, error) {
	for _, v := range tmpMetaData {
		if v.Name == name {
			return v.Id, nil
		}
	}
	return -1, fmt.Errorf("error: task name not found: %s", name)
}

func (w *Spearlet) ListTasks() []string {
	var tasks []string
	for _, v := range tmpMetaData {
		tasks = append(tasks, v.Name)
	}
	return tasks
}

func (w *Spearlet) ExecuteTaskByName(name string, wait bool, method string,
	data string) (string, error) {
	for _, v := range tmpMetaData {
		if v.Name == name {
			return w.ExecuteTask(v.Id, v.Type, wait, method, data)
		}
	}
	return "", fmt.Errorf("error: task name not found: %s", name)
}

func (w *Spearlet) metaDataToTaskCfg(meta TaskMetaData) *task.TaskConfig {
	randSrc := rand.NewSource(time.Now().UnixNano())
	randGen := rand.New(randSrc)
	name := fmt.Sprintf("task-%s-%d", meta.Name, randGen.Intn(10000))
	switch meta.Type {
	case task.TaskTypeDocker:
		return &task.TaskConfig{
			Name:     name,
			Cmd:      "/start",
			Args:     []string{},
			Image:    meta.ImageName,
			HostAddr: w.spearAddr,
		}
	case task.TaskTypeProcess:
		// go though search patch to find ExecName
		execName := ""
		for _, path := range w.cfg.SearchPath {
			log.Infof("Searching for exec %s in path %s", meta.ExecName, path)
			if _, err := os.Stat(filepath.Join(path, meta.ExecName)); err == nil {
				execName = filepath.Join(path, meta.ExecName)
				break
			}
		}
		if execName == "" {
			log.Errorf("Error: exec not found: %s", meta.Name)
			return nil
		}
		log.Infof("Using exec: %s", execName)
		return &task.TaskConfig{
			Name:     name,
			Cmd:      execName,
			Args:     []string{},
			Image:    "",
			HostAddr: w.spearAddr,
		}
	default:
		return nil
	}
}

func (w *Spearlet) ExecuteTask(taskId int64, funcType task.TaskType, wait bool,
	method string, data string) (string, error) {
	rt, err := task.GetTaskRuntime(funcType)
	if err != nil {
		return "", fmt.Errorf("error: %v", err)
	}

	// get metadata from taskId
	meta, ok := tmpMetaData[int(taskId)]
	if !ok {
		return "", fmt.Errorf("error: invalid task id: %d", taskId)
	}
	if meta.Type != funcType {
		return "", fmt.Errorf("error: invalid task type: %d, %+v",
			funcType, meta)
	}

	log.Infof("Using metadata: %+v", meta)

	cfg := w.metaDataToTaskCfg(meta)
	if cfg == nil {
		return "", fmt.Errorf("error: invalid task type: %d", funcType)
	}
	newTask, err := rt.CreateTask(cfg)
	if err != nil {
		return "", fmt.Errorf("error: %v", err)
	}
	err = w.commMgr.InstallToTask(newTask)
	if err != nil {
		return "", fmt.Errorf("error: %v", err)
	}

	log.Debugf("Starting task: %s", newTask.Name())
	newTask.Start()

	res := ""
	builder := flatbuffers.NewBuilder(512)
	methodOff := builder.CreateString(method)
	dataOff := builder.CreateString(data)
	custom.CustomRequestStart(builder)
	custom.CustomRequestAddMethodStr(builder, methodOff)
	custom.CustomRequestAddParamsStr(builder, dataOff)
	builder.Finish(custom.CustomRequestEnd(builder))

	if r, err := w.commMgr.SendOutgoingRPCRequest(newTask, transport.MethodCustom,
		builder.FinishedBytes()); err != nil {
		return "", fmt.Errorf("error: %v", err)
	} else {
		if len(r.ResponseBytes()) == 0 {
			return "", nil // no response
		}
		customResp := custom.GetRootAsCustomResponse(r.ResponseBytes(), 0)
		// marshal the result
		if resTmp, err := json.Marshal(customResp.DataBytes()); err != nil {
			return "", fmt.Errorf("error: %v", err)
		} else {
			res = string(resTmp)
		}
	}

	// terminate the task by sending a signal
	if err := w.commMgr.SendOutgoingRPCSignal(newTask, transport.SignalTerminate,
		[]byte{}); err != nil {
		return "", fmt.Errorf("error: %v", err)
	}

	if wait {
		// wait for the task to finish
		newTask.Wait()
	}

	return res, nil
}

func (w *Spearlet) addRoutes() {
	w.mux.HandleFunc("/health", func(resp http.ResponseWriter, req *http.Request) {
		resp.Write([]byte("OK"))
	})
	w.mux.HandleFunc("/", func(resp http.ResponseWriter, req *http.Request) {
		log.Debugf("Received request: %s", req.URL.Path)
		// get the function id
		taskId, err := funcId(req)
		if err != nil {
			respError(resp, fmt.Sprintf("Error: %v", err))
			return
		}

		// get the function type
		funcType, err := funcType(req)
		if err != nil {
			respError(resp, fmt.Sprintf("Error: %v", err))
			return
		}

		funcIsAsync, err := funcAsync(req)
		if err != nil {
			respError(resp, fmt.Sprintf("Error: %v", err))
			return
		}

		buf := make([]byte, common.MaxDataResponseSize)
		n, err := req.Body.Read(buf)
		if err != nil && err != io.EOF {
			log.Errorf("Error reading body: %v", err)
			respError(resp, fmt.Sprintf("Error: %v", err))
			return
		}
		res, err := w.ExecuteTask(taskId, funcType, !funcIsAsync, "handle",
			string(buf[:n]))
		if err != nil {
			respError(resp, fmt.Sprintf("Error: %v", err))
			return
		}
		resp.Write([]byte(res))
	})
}

func (w *Spearlet) StartServer() {
	log.Infof("Starting spearlet on %s:%s", w.cfg.Addr, w.cfg.Port)
	srv := &http.Server{
		Addr:    w.cfg.Addr + ":" + w.cfg.Port,
		Handler: w.mux,
	}
	w.srv = srv
	if err := srv.ListenAndServe(); err != nil {
		if err != http.ErrServerClosed {
			log.Errorf("Error: %v", err)
		} else {
			log.Info("Server closed")
		}
	}
}

func (w *Spearlet) Stop() {
	log.Debugf("Stopping spearlet")
	if w.srv != nil {
		w.srv.Shutdown(context.Background())
	}
	task.StopTaskRuntimes()
}

func SetLogLevel(lvl log.Level) {
	logLevel = lvl
	log.SetLevel(logLevel)
}

func init() {
	log.SetLevel(logLevel)
}

func respError(resp http.ResponseWriter, msg string) {
	resp.WriteHeader(http.StatusInternalServerError)
	resp.Write([]byte(msg))
}
