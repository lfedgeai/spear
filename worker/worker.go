package worker

import (
	"encoding/json"
	"fmt"
	"io"
	"net/http"

	log "github.com/sirupsen/logrus"

	"github.com/lfedgeai/spear/pkg/common"
	"github.com/lfedgeai/spear/pkg/rpc"
	"github.com/lfedgeai/spear/worker/hostcalls"
	"github.com/lfedgeai/spear/worker/hostcalls/openai"
	"github.com/lfedgeai/spear/worker/task"
)

var (
	logLevel = log.InfoLevel
)

type WorkerConfig struct {
	Addr string
	Port string
}

type Worker struct {
	cfg *WorkerConfig
	mux *http.ServeMux

	hc *hostcalls.HostCalls
}

// NewWorkerConfig creates a new WorkerConfig
func NewWorkerConfig(addr, port string) *WorkerConfig {
	return &WorkerConfig{
		Addr: addr,
		Port: port,
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
	for _, hc := range openai.Hostcalls {
		w.hc.RegisterHostCall(hc)
	}
}

func (w *Worker) addRoutes() {
	w.mux.HandleFunc("/health", func(resp http.ResponseWriter, req *http.Request) {
		resp.Write([]byte("OK"))
	})
	w.mux.HandleFunc("/", func(resp http.ResponseWriter, req *http.Request) {
		rt, err := task.NewTaskRuntime(task.TaskTypeProcess)
		if err != nil {
			respError(resp, fmt.Sprintf("Error: %v", err))
			return
		}
		task, err := rt.CreateTask(&task.TaskConfig{
			Name: "dummy_task",
			Cmd:  "./dummy_task",
			Args: []string{},
		})
		if err != nil {
			respError(resp, fmt.Sprintf("Error: %v", err))
			return
		}
		w.hc.InstallToTask(task)

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
	if err := http.ListenAndServe(w.cfg.Addr+":"+w.cfg.Port, w.mux); err != nil {
		fmt.Println("Error:", err)
	}
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
