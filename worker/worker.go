package worker

import (
	"bytes"
	"encoding/json"
	"fmt"
	"io"
	"net/http"
	"os"

	log "github.com/sirupsen/logrus"

	"github.com/lfedgeai/spear/pkg/openai"
	"github.com/lfedgeai/spear/pkg/rpc"
	"github.com/lfedgeai/spear/worker/hostcalls"
	"github.com/lfedgeai/spear/worker/task"
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
	w.hc.RegisterHostCall("chat.completion", func(args interface{}) (interface{}, error) {
		log.Infof("Executing hostcall \"%s\" with args %v", "chat.completion", args)
		// verify the type of args is ChatCompletionRequest
		// use json marshal and unmarshal to verify the type
		jsonBytes, err := json.Marshal(args)
		if err != nil {
			return nil, fmt.Errorf("error marshalling args: %v", err)
		}
		chatReq := openai.ChatCompletionRequest{}
		err = json.Unmarshal(jsonBytes, &chatReq)
		if err != nil {
			return nil, fmt.Errorf("error unmarshalling args: %v", err)
		}

		log.Infof("Chat request: %s", string(jsonBytes))
		// create a https request to https://api.openai.com/v1/chat/completions and use b as the request body
		req, err := http.NewRequest("POST", "https://api.openai.com/v1/chat/completions", bytes.NewBuffer(jsonBytes))
		if err != nil {
			return nil, fmt.Errorf("error creating request: %v", err)
		}

		// get api key from environment variable
		apiKey := os.Getenv("OPENAI_API_KEY")
		// set the headers
		req.Header.Set("Content-Type", "application/json")
		req.Header.Set("Authorization", fmt.Sprintf("Bearer %s", apiKey))
		// send the request
		client := &http.Client{}
		resp, err := client.Do(req)
		if err != nil {
			return nil, fmt.Errorf("error sending request: %v", err)
		}
		// read the response
		buf := make([]byte, 4096)
		n, err := resp.Body.Read(buf)
		if err != nil && err != io.EOF {
			return nil, fmt.Errorf("error reading response: %v", err)
		}
		// print the response
		log.Infof("Response: %s", buf[:n])
		// return the response
		return "OK", nil
	})
}

func (w *Worker) addRoutes() {
	w.mux.HandleFunc("/health", func(resp http.ResponseWriter, req *http.Request) {
		resp.Write([]byte("OK"))
	})
	w.mux.HandleFunc("/", func(resp http.ResponseWriter, req *http.Request) {
		// print all headers
		log.Infof("Headers: %v", req.Header)

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
		buf := make([]byte, 4096)
		n, err := req.Body.Read(buf)
		if err != nil && err != io.EOF {
			respError(resp, fmt.Sprintf("Error: %v", err))
			return
		}
		workerReq := rpc.JsonRPCRequest{
			Version: "2.0",
			Method:  "handle",
			Params:  []interface{}{string(buf[:n])},
			ID:      "1",
		}
		b, err := workerReq.Marshal()
		if err != nil {
			respError(resp, fmt.Sprintf("Error: %v", err))
			return
		}
		in <- b
		in <- []byte("\n")

	})
}

func (w *Worker) Run() {
	log.Infof("Starting worker on %s:%s", w.cfg.Addr, w.cfg.Port)
	if err := http.ListenAndServe(w.cfg.Addr+":"+w.cfg.Port, w.mux); err != nil {
		fmt.Println("Error:", err)
	}
}

func init() {
	log.SetLevel(log.DebugLevel)
}

func respError(resp http.ResponseWriter, msg string) {
	resp.WriteHeader(http.StatusInternalServerError)
	resp.Write([]byte(msg))
}
