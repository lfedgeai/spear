package worker

import (
	"context"
	"encoding/json"
	"fmt"
	"io"
	"math/rand"
	"net/http"
	"strconv"
	"time"

	log "github.com/sirupsen/logrus"

	"github.com/lfedgeai/spear/pkg/common"
	hc "github.com/lfedgeai/spear/worker/hostcalls"
	hostcalls "github.com/lfedgeai/spear/worker/hostcalls/common"
	"github.com/lfedgeai/spear/worker/task"
	_ "github.com/lfedgeai/spear/worker/tools"
)

var (
	logLevel = log.InfoLevel
)

type WorkerConfig struct {
	Addr string
	Port string

	// Search Path
	SearchPath []string

	// Debug
	Debug          bool
	LocalExecution bool

	SpearAddr string
}

type Worker struct {
	cfg *WorkerConfig
	mux *http.ServeMux
	srv *http.Server

	SearchPaths []string
	hc          *hostcalls.HostCalls
	commMgr     *hostcalls.CommunicationManager

	spearAddr string
}

type TaskMetaData struct {
	Id    int64
	Type  task.TaskType
	Image string
	Name  string
}

var (
	tmpMetaData = map[int]TaskMetaData{
		1: {
			Id:    1,
			Type:  task.TaskTypeDocker,
			Image: "dummy",
			Name:  "dummy",
		},
		2: {
			Id:    2,
			Type:  task.TaskTypeDocker,
			Image: "voice_chat",
			Name:  "voice_chat",
		},
		3: {
			Id:    3,
			Type:  task.TaskTypeDocker,
			Image: "gen_image",
			Name:  "gen_image",
		},
		4: {
			Id:    4,
			Type:  task.TaskTypeDocker,
			Image: "pychat",
			Name:  "pychat",
		},
		5: {
			Id:    5,
			Type:  task.TaskTypeDocker,
			Image: "pytools",
			Name:  "pytools",
		},
		6: {
			Id:    6,
			Type:  task.TaskTypeDocker,
			Image: "pyconversation",
			Name:  "pyconversation",
		},
		7: {
			Id:    7,
			Type:  task.TaskTypeDocker,
			Image: "pydummy",
			Name:  "pydummy",
		},
	}
)

// NewServeWorkerConfig creates a new WorkerConfig
func NewServeWorkerConfig(addr, port string, spath []string, debug bool, spearAddr string) *WorkerConfig {
	return &WorkerConfig{
		Addr:           addr,
		Port:           port,
		SearchPath:     spath,
		Debug:          debug,
		LocalExecution: false,
		SpearAddr:      spearAddr,
	}
}

func NewExecWorkerConfig(debug bool, spearAddr string) *WorkerConfig {
	return &WorkerConfig{
		Addr:           "",
		Port:           "",
		Debug:          debug,
		LocalExecution: true,
		SpearAddr:      spearAddr,
	}
}

func NewWorker(cfg *WorkerConfig) *Worker {
	w := &Worker{
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

func (w *Worker) Initialize() {
	w.addRoutes()
	w.addHostCalls()
	w.initializeRuntimes()
	go w.hc.Run()
}

func (w *Worker) addHostCalls() {
	for _, hc := range hc.Hostcalls {
		w.hc.RegisterHostCall(hc)
	}
}

func (w *Worker) initializeRuntimes() {
	cfg := &task.TaskRuntimeConfig{
		Debug:         w.cfg.Debug,
		Cleanup:       true,
		StartServices: true,
	}
	task.RegisterSupportedTaskType(task.TaskTypeDocker)
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
		return task.TaskTypeUnknown, fmt.Errorf("missing %s header", HeaderFuncType)
	}

	// convert runtime to int
	i, err := strconv.Atoi(runtime)
	if err != nil {
		return task.TaskTypeUnknown, fmt.Errorf("error parsing %s header: %v", HeaderFuncType, err)
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
		return task.TaskTypeUnknown, fmt.Errorf("invalid %s header: %s", HeaderFuncType, runtime)
	}
}

func (w *Worker) LookupTaskId(name string) (int64, error) {
	for _, v := range tmpMetaData {
		if v.Name == name {
			return v.Id, nil
		}
	}
	return -1, fmt.Errorf("error: task name not found: %s", name)
}

func (w *Worker) ListTasks() []string {
	var tasks []string
	for _, v := range tmpMetaData {
		tasks = append(tasks, v.Name)
	}
	return tasks
}

func (w *Worker) ExecuteTask(taskId int64, funcType task.TaskType, wait bool, method string, data string) (string, error) {
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
		return "", fmt.Errorf("error: invalid task type: %d", funcType)
	}

	randSrc := rand.NewSource(time.Now().UnixNano())
	randGen := rand.New(randSrc)
	newTask, err := rt.CreateTask(&task.TaskConfig{
		Name:   fmt.Sprintf("task-%s-%d", meta.Name, randGen.Intn(10000)),
		Cmd:    "/start", //"sh", //"./dummy_task",
		Args:   []string{},
		Image:  meta.Image,
		HostAddr: w.spearAddr,
	})
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
	if r, err := w.commMgr.SendOutgoingRPCRequest(newTask, method, data); err != nil {
		return "", fmt.Errorf("error: %v", err)
	} else {
		// marshal the result
		if resTmp, err := json.Marshal(r.Result); err != nil {
			return "", fmt.Errorf("error: %v", err)
		} else {
			res = string(resTmp)
		}
	}

	if wait {
		// wait for the task to finish
		newTask.Wait()
	}

	return res, nil
}

func (w *Worker) addRoutes() {
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
		res, err := w.ExecuteTask(taskId, funcType, !funcIsAsync, "handle", string(buf[:n]))
		if err != nil {
			respError(resp, fmt.Sprintf("Error: %v", err))
			return
		}
		resp.Write([]byte(res))
	})
}

func (w *Worker) StartServer() {
	log.Infof("Starting worker on %s:%s", w.cfg.Addr, w.cfg.Port)
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

func (w *Worker) Stop() {
	log.Debugf("Stopping worker")
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
