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
	"github.com/lfedgeai/spear/pkg/rpc"
	hc "github.com/lfedgeai/spear/worker/hostcalls"
	hostcalls "github.com/lfedgeai/spear/worker/hostcalls/common"
	"github.com/lfedgeai/spear/worker/task"
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
	Debug bool
}

type Worker struct {
	cfg *WorkerConfig
	mux *http.ServeMux
	srv *http.Server

	SearchPaths []string
	hc          *hostcalls.HostCalls
}

// NewWorkerConfig creates a new WorkerConfig
func NewWorkerConfig(addr, port string, spath []string, debug bool) *WorkerConfig {
	return &WorkerConfig{
		Addr:       addr,
		Port:       port,
		SearchPath: spath,
		Debug:      debug,
	}
}

func NewWorker(cfg *WorkerConfig) *Worker {
	hc := hostcalls.NewHostCalls()
	w := &Worker{
		cfg: cfg,
		mux: http.NewServeMux(),
		hc:  hc,
	}
	return w
}

func (w *Worker) Init() {
	w.addRoutes()
	w.addHostCalls()
}

func (w *Worker) addHostCalls() {
	for _, hc := range hc.Hostcalls {
		w.hc.RegisterHostCall(hc)
	}
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
	}
)

func (w *Worker) addRoutes() {
	w.mux.HandleFunc("/health", func(resp http.ResponseWriter, req *http.Request) {
		resp.Write([]byte("OK"))
	})
	w.mux.HandleFunc("/", func(resp http.ResponseWriter, req *http.Request) {
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

		rt, err := task.GetTaskRuntime(funcType, &task.TaskRuntimeConfig{
			Debug: w.cfg.Debug,
		})
		if err != nil {
			respError(resp, fmt.Sprintf("Error: %v", err))
			return
		}

		// get metadata from taskId
		// TODO: implement me later
		meta, ok := tmpMetaData[int(taskId)]
		if !ok {
			respError(resp, fmt.Sprintf("Error: invalid task id: %d", taskId))
			return
		}
		if meta.Type != funcType {
			respError(resp, fmt.Sprintf("Error: invalid task type: %d", funcType))
			return
		}

		randSrc := rand.NewSource(time.Now().UnixNano())
		randGen := rand.New(randSrc)
		task, err := rt.CreateTask(&task.TaskConfig{
			Name:  fmt.Sprintf("task-%s-%d", meta.Name, randGen.Intn(10000)),
			Cmd:   "/start", //"sh", //"./dummy_task",
			Args:  []string{},
			Image: meta.Image,
		})
		if err != nil {
			respError(resp, fmt.Sprintf("Error: %v", err))
			return
		}
		err = w.hc.InstallToTask(task)
		if err != nil {
			respError(resp, fmt.Sprintf("Error: %v", err))
			return
		}

		log.Debugf("Starting task: %s", task.Name())
		task.Start()

		// get input output channels
		in, _, err := task.CommChannels()
		if err != nil {
			respError(resp, fmt.Sprintf("Error: %v", err))
			return
		}

		// write to the input channel
		// read the body
		buf := make([]byte, common.MaxDataResponseSize)
		n, err := req.Body.Read(buf)
		if err != nil && err != io.EOF {
			log.Errorf("Error reading body: %v", err)
			respError(resp, fmt.Sprintf("Error: %v", err))
			return
		}
		method := "handle"
		id := json.Number("0")
		workerReq := rpc.JsonRPCRequest{
			Version: "2.0",
			Method:  &method,
			Params:  []interface{}{string(buf[:n])},
			ID:      &id,
		}
		b, err := workerReq.Marshal()
		if err != nil {
			log.Errorf("Error marshalling request: %v", err)
			respError(resp, fmt.Sprintf("Error: %v", err))
			return
		}
		// output b + '\n' to the input channel
		in <- append(b, '\n')

		task.Wait()
	})
}

func (w *Worker) Run() {
	log.Infof("Starting worker on %s:%s", w.cfg.Addr, w.cfg.Port)
	srv := &http.Server{
		Addr:    w.cfg.Addr + ":" + w.cfg.Port,
		Handler: w.mux,
	}
	w.srv = srv
	if err := srv.ListenAndServe(); err != nil {
		log.Infof("Error: %v", err)
	}
}

func (w *Worker) Stop() {
	log.Infof("Stopping worker")
	w.srv.Shutdown(context.Background())
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
