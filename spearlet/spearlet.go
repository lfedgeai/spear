package spearlet

import (
	"context"
	"fmt"
	"io"
	"math/rand"
	"net/http"
	"os"
	"path/filepath"
	"strconv"
	"time"

	"github.com/gin-gonic/gin"
	flatbuffers "github.com/google/flatbuffers/go"
	log "github.com/sirupsen/logrus"

	"github.com/lfedgeai/spear/pkg/common"
	"github.com/lfedgeai/spear/pkg/spear/proto/custom"
	"github.com/lfedgeai/spear/pkg/spear/proto/transport"
	hc "github.com/lfedgeai/spear/spearlet/hostcalls"
	hostcalls "github.com/lfedgeai/spear/spearlet/hostcalls/common"
	"github.com/lfedgeai/spear/spearlet/task"
	_ "github.com/lfedgeai/spear/spearlet/tools"

	"github.com/docker/docker/client"
	"github.com/gorilla/websocket"
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
	Debug bool

	SpearAddr string

	// backend service
	StartBackendServices bool

	CertFile string
	KeyFile  string
}

type Spearlet struct {
	cfg *SpearletConfig
	mux *http.ServeMux
	srv *http.Server

	hc      *hostcalls.HostCalls
	commMgr *hostcalls.CommunicationManager
	mQueues map[task.Task]map[uint16]chan task.Message

	spearAddr string

	isSSL    bool
	certFile string
	keyFile  string

	streamUpgrader websocket.Upgrader
}

type TaskMetaData struct {
	Id        int64
	Type      task.TaskType
	ImageName string
	ExecName  string
	Name      string
}

var (
	tmpMetaData = map[int64]TaskMetaData{
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
	spearAddr string, certFile string, keyFile string,
	startBackendService bool) (*SpearletConfig, error) {
	if certFile != "" && keyFile == "" || certFile == "" && keyFile != "" {
		return nil, fmt.Errorf("both cert and key files must be provided")
	}
	return &SpearletConfig{
		Addr:                 addr,
		Port:                 port,
		SearchPath:           spath,
		Debug:                debug,
		SpearAddr:            spearAddr,
		StartBackendServices: startBackendService,
		CertFile:             certFile,
		KeyFile:              keyFile,
	}, nil
}

func NewExecSpearletConfig(debug bool, spearAddr string, spath []string,
	startBackendServices bool) *SpearletConfig {
	return &SpearletConfig{
		Addr:                 "",
		Port:                 "",
		SearchPath:           spath,
		Debug:                debug,
		SpearAddr:            spearAddr,
		StartBackendServices: startBackendServices,
	}
}

func NewSpearlet(cfg *SpearletConfig) *Spearlet {
	w := &Spearlet{
		cfg:       cfg,
		mux:       http.NewServeMux(),
		hc:        nil,
		commMgr:   hostcalls.NewCommunicationManager(),
		mQueues:   map[task.Task]map[uint16]chan task.Message{},
		spearAddr: cfg.SpearAddr,
		streamUpgrader: websocket.Upgrader{
			ReadBufferSize:  1024 * 4,
			WriteBufferSize: 1024 * 4,
			CheckOrigin: func(r *http.Request) bool {
				return true
			},
		},
	}
	if cfg.CertFile != "" && cfg.KeyFile != "" {
		w.isSSL = true
		w.certFile = cfg.CertFile
		w.keyFile = cfg.KeyFile
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
		StartServices: w.cfg.StartBackendServices,
	}
	task.RegisterSupportedTaskType(task.TaskTypeDocker)
	task.RegisterSupportedTaskType(task.TaskTypeProcess)
	task.InitTaskRuntimes(cfg)
}

func isStreamingRequest(req *http.Request) bool {
	headers := req.Header
	// get the streaming flag from the headers
	streaming := headers.Get(HeaderStreamingFunction)
	if streaming == "" {
		return false
	}

	// convert streaming to bool
	b, err := strconv.ParseBool(streaming)
	if err != nil {
		log.Errorf("error parsing %s header: %v", HeaderStreamingFunction, err)
		return false
	}

	return b
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
		return false, fmt.Errorf("error parsing %s header: %v",
			HeaderFuncAsync, err)
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
		return -1, fmt.Errorf("error parsing %s header: %v",
			HeaderFuncId, err)
	}

	return i, nil
}

func funcName(req *http.Request) (string, error) {
	// get request headers
	headers := req.Header
	// get the name from the headers
	name := headers.Get(HeaderFuncName)
	if name == "" {
		return "", fmt.Errorf("missing %s header", HeaderFuncName)
	}

	return name, nil
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

func (w *Spearlet) CommunicationManager() *hostcalls.CommunicationManager {
	return w.commMgr
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

func (w *Spearlet) RunTask(funcId int64, funcName string, funcType task.TaskType,
	method string, data string, inStream chan task.Message,
	terminate bool, wait bool) (
	respData string, respStream chan task.Message, err error) {
	t, respData, respStream, err := w.ExecuteTask(funcId, funcName, funcType, method, data, inStream)
	if err != nil {
		return "", nil, err
	}
	if terminate {
		if err := w.commMgr.SendOutgoingRPCSignal(t, transport.SignalTerminate,
			[]byte{}); err != nil {
			return "", nil, fmt.Errorf("error: %v", err)
		}
	}
	if wait {
		if _, err := t.Wait(); err != nil {
			log.Warnf("Error waiting for task: %v", err)
		}
	}
	return respData, respStream, nil
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
			WorkDir:  "",
			HostAddr: w.spearAddr,
		}
	case task.TaskTypeProcess:
		// go though search patch to find ExecName
		execName := ""
		execPath := ""
		for _, path := range w.cfg.SearchPath {
			log.Infof("Searching for exec %s in path %s", meta.ExecName, path)
			if _, err := os.Stat(filepath.Join(path, meta.ExecName)); err == nil {
				execName = filepath.Join(path, meta.ExecName)
				execPath = path
				break
			}
		}
		if execName == "" || execPath == "" {
			log.Errorf("Error: exec name %s and path %s not found",
				meta.ExecName, execPath)
			return nil
		}
		log.Infof("Using exec: %s", execName)
		return &task.TaskConfig{
			Name:     name,
			Cmd:      execName,
			Args:     []string{},
			Image:    "",
			WorkDir:  execPath,
			HostAddr: w.spearAddr,
		}
	default:
		return nil
	}
}

func (w *Spearlet) ExecuteTaskByName(taskName string, funcType task.TaskType, method string,
	reqData string, reqStream chan task.Message) (t task.Task,
	respData string, respStream chan task.Message,
	err error) {
	var fakeMeta TaskMetaData

	if _, ok := task.GlobalTaskRuntimes[funcType]; !ok {
		return nil, "", nil, fmt.Errorf("error: task runtime not found: %d",
			funcType)
	}

	switch funcType {
	case task.TaskTypeDocker:
		// search if the docker image exists
		// if not, return error
		cli, err := client.NewClientWithOpts(client.FromEnv)
		if err != nil {
			return nil, "", nil, fmt.Errorf("error: %v", err)
		}

		_, _, err = cli.ImageInspectWithRaw(context.Background(), taskName)
		if err != nil {
			return nil, "", nil, fmt.Errorf("error: %v", err)
		}

		fakeMeta = TaskMetaData{
			Id:        -1,
			Type:      task.TaskTypeDocker,
			ImageName: taskName,
			Name:      taskName,
		}
	case task.TaskTypeProcess:
		fakeMeta = TaskMetaData{
			Id:       -1,
			Type:     task.TaskTypeProcess,
			ExecName: taskName,
			Name:     taskName,
		}
	case task.TaskTypeDylib:
		panic("not implemented")
	case task.TaskTypeWasm:
		panic("not implemented")
	default:
		panic("invalid task type")
	}

	log.Infof("Using metadata: %+v", fakeMeta)

	cfg := w.metaDataToTaskCfg(fakeMeta)
	if cfg == nil {
		return nil, "", nil, fmt.Errorf("error: invalid task type: %d",
			funcType)
	}

	rt, err := task.GetTaskRuntime(funcType)
	if err != nil {
		return nil, "", nil, fmt.Errorf("error: %v", err)
	}

	newTask, err := rt.CreateTask(cfg)
	if err != nil {
		return nil, "", nil, fmt.Errorf("error: %v", err)
	}
	err = w.commMgr.InstallToTask(newTask)
	if err != nil {
		return nil, "", nil, fmt.Errorf("error: %v", err)
	}

	log.Debugf("Starting task: %s", newTask.Name())
	newTask.Start()

	reqMQueueID := uint16(0)
	w.mQueues[newTask] = map[uint16]chan task.Message{
		reqMQueueID: make(chan task.Message, 1024),
	}
	respMQueueID := uint16(1)
	w.mQueues[newTask] = map[uint16]chan task.Message{
		respMQueueID: make(chan task.Message, 1024),
	}

	builder := flatbuffers.NewBuilder(512)
	methodOff := builder.CreateString(method)

	if reqStream == nil {
		dataOff := builder.CreateString(reqData)

		custom.NormalRequestInfoStart(builder)
		custom.NormalRequestInfoAddParamsStr(builder, dataOff)
		infoOff := custom.NormalRequestInfoEnd(builder)

		custom.CustomRequestStart(builder)
		custom.CustomRequestAddMethodStr(builder, methodOff)
		custom.CustomRequestAddRequestInfoType(builder,
			custom.RequestInfoNormalRequestInfo)
		custom.CustomRequestAddRequestInfo(builder, infoOff)
		builder.Finish(custom.CustomRequestEnd(builder))
	} else {
		custom.StreamRequestInfoStart(builder)
		custom.StreamRequestInfoAddInQueueId(builder, int32(reqMQueueID))
		custom.StreamRequestInfoAddOutQueueId(builder, int32(respMQueueID))
		infoOff := custom.StreamRequestInfoEnd(builder)

		custom.CustomRequestStart(builder)
		custom.CustomRequestAddMethodStr(builder, methodOff)
		custom.CustomRequestAddRequestInfoType(builder,
			custom.RequestInfoStreamRequestInfo)
		custom.CustomRequestAddRequestInfo(builder, infoOff)
		builder.Finish(custom.CustomRequestEnd(builder))
	}

	if reqStream != nil {
		go func() {
			for msg := range reqStream {
				w.mQueues[newTask][reqMQueueID] <- msg
			}
		}()
	}

	if r, err := w.commMgr.SendOutgoingRPCRequest(newTask,
		transport.MethodCustom,
		builder.FinishedBytes()); err != nil {
		return nil, "", nil, fmt.Errorf("error: %v", err)
	} else {
		if len(r.ResponseBytes()) == 0 {
			return newTask, "", nil, nil
		}
		customResp := custom.GetRootAsCustomResponse(r.ResponseBytes(), 0)

		if customResp.ReturnStream() {
			// stream return
			queueId := respMQueueID
			// streaming response
			if _, ok := w.mQueues[newTask][uint16(queueId)]; !ok {
				return nil, "", nil, fmt.Errorf("error: queue not found: %d",
					queueId)
			}

			return newTask, "", w.mQueues[newTask][uint16(queueId)], nil
		} else {
			customRespData := customResp.DataBytes()
			return newTask, string(customRespData), nil, nil
		}
	}
}

func (w *Spearlet) ExecuteTaskById(taskId int64, funcType task.TaskType, method string,
	reqData string, reqStream chan task.Message) (t task.Task,
	respData string,
	respStream chan task.Message,
	err error) {
	// get metadata from taskId
	meta, ok := tmpMetaData[taskId]
	if !ok {
		return nil, "", nil, fmt.Errorf("error: invalid task id: %d",
			taskId)
	}
	if funcType == task.TaskTypeUnknown {
		funcType = meta.Type
	}
	if meta.Type != funcType {
		return nil, "", nil, fmt.Errorf("error: invalid task type: %d, %+v",
			funcType, meta)
	}

	log.Infof("Using metadata: %+v", meta)

	cfg := w.metaDataToTaskCfg(meta)
	if cfg == nil {
		return nil, "", nil, fmt.Errorf("error: invalid task type: %d",
			funcType)
	}

	rt, err := task.GetTaskRuntime(funcType)
	if err != nil {
		return nil, "", nil, fmt.Errorf("error: %v", err)
	}

	newTask, err := rt.CreateTask(cfg)
	if err != nil {
		return nil, "", nil, fmt.Errorf("error: %v", err)
	}
	err = w.commMgr.InstallToTask(newTask)
	if err != nil {
		return nil, "", nil, fmt.Errorf("error: %v", err)
	}

	log.Debugf("Starting task: %s", newTask.Name())
	newTask.Start()

	reqMQueueID := uint16(0)
	w.mQueues[newTask] = map[uint16]chan task.Message{
		reqMQueueID: make(chan task.Message, 1024),
	}
	respMQueueID := uint16(1)
	w.mQueues[newTask] = map[uint16]chan task.Message{
		respMQueueID: make(chan task.Message, 1024),
	}

	builder := flatbuffers.NewBuilder(512)
	methodOff := builder.CreateString(method)

	if reqStream == nil {
		dataOff := builder.CreateString(reqData)

		custom.NormalRequestInfoStart(builder)
		custom.NormalRequestInfoAddParamsStr(builder, dataOff)
		infoOff := custom.NormalRequestInfoEnd(builder)

		custom.CustomRequestStart(builder)
		custom.CustomRequestAddMethodStr(builder, methodOff)
		custom.CustomRequestAddRequestInfoType(builder,
			custom.RequestInfoNormalRequestInfo)
		custom.CustomRequestAddRequestInfo(builder, infoOff)
		builder.Finish(custom.CustomRequestEnd(builder))
	} else {
		custom.StreamRequestInfoStart(builder)
		custom.StreamRequestInfoAddInQueueId(builder, int32(reqMQueueID))
		custom.StreamRequestInfoAddOutQueueId(builder, int32(respMQueueID))
		infoOff := custom.StreamRequestInfoEnd(builder)

		custom.CustomRequestStart(builder)
		custom.CustomRequestAddMethodStr(builder, methodOff)
		custom.CustomRequestAddRequestInfoType(builder,
			custom.RequestInfoStreamRequestInfo)
		custom.CustomRequestAddRequestInfo(builder, infoOff)
		builder.Finish(custom.CustomRequestEnd(builder))
	}

	if reqStream != nil {
		go func() {
			for msg := range reqStream {
				w.mQueues[newTask][reqMQueueID] <- msg
			}
		}()
	}

	if r, err := w.commMgr.SendOutgoingRPCRequest(newTask,
		transport.MethodCustom,
		builder.FinishedBytes()); err != nil {
		return nil, "", nil, fmt.Errorf("error: %v", err)
	} else {
		if len(r.ResponseBytes()) == 0 {
			return newTask, "", nil, nil
		}
		customResp := custom.GetRootAsCustomResponse(r.ResponseBytes(), 0)

		if customResp.ReturnStream() {
			// stream return
			queueId := respMQueueID
			// streaming response
			if _, ok := w.mQueues[newTask][uint16(queueId)]; !ok {
				return nil, "", nil, fmt.Errorf("error: queue not found: %d",
					queueId)
			}

			return newTask, "", w.mQueues[newTask][uint16(queueId)], nil
		} else {
			customRespData := customResp.DataBytes()
			return newTask, string(customRespData), nil, nil
		}
	}
}

func (w *Spearlet) ExecuteTask(funcId int64, funcName string, funcType task.TaskType,
	method string,
	data string, inStream chan task.Message) (t task.Task, respData string,
	respStream chan task.Message, err error) {
	if funcId >= 0 {
		return w.ExecuteTaskById(funcId, funcType, method, data, inStream)
	}
	if funcName != "" {
		return w.ExecuteTaskByName(funcName, funcType, method, data, inStream)
	}
	return nil, "", nil, fmt.Errorf("error: invalid task id or name")
}

func (w *Spearlet) handler(req *http.Request, resp http.ResponseWriter) {
	var inData string
	var inStream chan task.Message
	var conn *websocket.Conn
	var err error

	upgraded := false

	streamingReq := isStreamingRequest(req)

	if streamingReq {
		log.Infof("Streaming request")
		conn, err = w.streamUpgrader.Upgrade(resp, req, nil)
		if err != nil {
			respError(resp, fmt.Sprintf("Error: %v", err))
			return
		}
		defer conn.Close()
		upgraded = true

		inStream = make(chan task.Message, 1024)
		go func() {
			defer close(inStream)
			for {
				_, msg, err := conn.ReadMessage()
				if err != nil {
					log.Errorf("Error reading message: %v", err)
					return
				}
				inStream <- task.Message(msg)
			}
		}()
	} else {
		buf := make([]byte, common.MaxDataResponseSize)
		n, err := req.Body.Read(buf)
		if err != nil && err != io.EOF {
			log.Errorf("Error reading body: %v", err)
			respError(resp, fmt.Sprintf("Error: %v", err))
			return
		}
		inData = string(buf[:n])
	}

	// get the function type
	funcType, err := funcType(req)
	if err != nil {
		respError(resp, fmt.Sprintf("Error: %v", err))
		return
	}

	// get the function id
	taskId, errTaskId := funcId(req)
	taskName, errTaskName := funcName(req)
	if errTaskId != nil && errTaskName != nil {
		respError(resp, fmt.Sprintf("Error: taskid or taskname is required"))
		return
	}

	t, outData, outStream, err := w.ExecuteTask(taskId, taskName, funcType, "handle",
		inData, inStream)
	if err != nil {
		respError(resp, fmt.Sprintf("Error: %v", err))
		return
	}

	if outStream != nil {
		log.Infof("Streaming response")
		if !upgraded {
			conn, err = w.streamUpgrader.Upgrade(resp, req, nil)
			if err != nil {
				respError(resp, fmt.Sprintf("Error: %v", err))
				return
			}
			defer conn.Close()
		}

		for msg := range outStream {
			err := conn.WriteMessage(websocket.TextMessage, []byte(msg))
			if err != nil {
				log.Errorf("Error writing message: %v", err)
				return
			}
		}
	} else {
		resp.Write([]byte(outData))
	}

	log.Infof("Terminating task %v", t)
	// terminate the task by sending a signal
	if err := w.commMgr.SendOutgoingRPCSignal(t,
		transport.SignalTerminate,
		[]byte{}); err != nil {
		log.Warnf("Error: %v", err)
	}
	if err := t.Stop(); err != nil {
		log.Warnf("Error stopping task: %v", err)
	}
}

func (w *Spearlet) addRoutes() {
	w.mux.HandleFunc("/health", func(resp http.ResponseWriter,
		req *http.Request) {
		resp.Write([]byte("OK"))
	})
	w.mux.HandleFunc("/", func(resp http.ResponseWriter,
		req *http.Request) {
		log.Debugf("Received request: %s", req.URL.Path)
		w.handler(req, resp)
	})
}

func (w *Spearlet) StartProviderService() {
	log.Infof("Starting provider service")
	// setup gin
	r := gin.Default()
	r.GET("/model", func(c *gin.Context) {
		// list all APIEndpointMap
		c.JSON(http.StatusOK, hostcalls.APIEndpointMap)
	})
	r.GET("/model/:type", func(c *gin.Context) {
		// list all APIEndpointMap with function type `type`
		typ := c.Param("type")
		// convert to int
		t, err := strconv.Atoi(typ)
		if err != nil {
			c.JSON(http.StatusBadRequest, gin.H{"error": "invalid type"})
			return
		}
		if _, ok := hostcalls.APIEndpointMap[hostcalls.OpenAIFunctionType(t)]; !ok {
			c.JSON(http.StatusBadRequest, gin.H{"error": "invalid type"})
			return
		}
		c.JSON(http.StatusOK,
			hostcalls.APIEndpointMap[hostcalls.OpenAIFunctionType(t)])
	})
	r.POST("/model/:type", func(c *gin.Context) {
		// add or update APIEndpointMap with function type `type` and name `name`
		typ := c.Param("type")
		// convert to int
		t, err := strconv.Atoi(typ)
		if err != nil {
			c.JSON(http.StatusBadRequest, gin.H{"error": "invalid type"})
			return
		}
		// get the body
		var body hostcalls.APIEndpointInfo
		if err := c.BindJSON(&body); err != nil {
			c.JSON(http.StatusBadRequest, gin.H{"error": "invalid body"})
			return
		}
		if _, ok := hostcalls.APIEndpointMap[hostcalls.OpenAIFunctionType(t)]; !ok {
			hostcalls.APIEndpointMap[hostcalls.OpenAIFunctionType(t)] =
				[]hostcalls.APIEndpointInfo{}
		}
		hostcalls.APIEndpointMap[hostcalls.OpenAIFunctionType(t)] = append(
			hostcalls.APIEndpointMap[hostcalls.OpenAIFunctionType(t)], body)
		c.JSON(http.StatusOK, gin.H{"status": "success"})
	})

	go func() {
		// convert port to number and increment by 1
		port, err := strconv.Atoi(w.cfg.Port)
		if err != nil {
			log.Fatalf("Error: %v", err)
		}
		port++
		log.Infof("Starting ProviderService server on port %d", port)
		if err := r.Run(fmt.Sprintf("%s:%d", w.cfg.Addr, port)); err != nil {
			log.Fatalf("Failed to start gin server: %v", err)
		}
	}()
}

func (w *Spearlet) StartServer() {
	log.Infof("Starting spearlet on %s:%s", w.cfg.Addr, w.cfg.Port)
	srv := &http.Server{
		Addr:    w.cfg.Addr + ":" + w.cfg.Port,
		Handler: w.mux,
	}
	w.srv = srv
	if w.isSSL {
		log.Infof("SSL Enabled")
		if err := srv.ListenAndServeTLS(w.certFile, w.keyFile); err != nil {
			log.Errorf("Error: %v", err)
		}
	} else {
		log.Infof("SSL Disabled")
		if err := srv.ListenAndServe(); err != nil {
			if err != http.ErrServerClosed {
				log.Errorf("Error: %v", err)
			} else {
				log.Info("Server closed")
			}
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
