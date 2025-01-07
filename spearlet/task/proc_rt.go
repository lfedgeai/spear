package task

import (
	"encoding/binary"
	"fmt"
	"math/rand"
	"net"
	"os"
	"os/exec"
	"time"

	log "github.com/sirupsen/logrus"
)

// implement TaskRuntimeProcess
type ProcessTaskRuntime struct {
	listenPort string
	tasks      map[TaskID]Task
}

const (
	ProcessRuntimeTcpListenPortBase = 9100
)

var (
	ProcessRuntimeTcpListenPort = ""
)

func NewProcessTaskRuntime() *ProcessTaskRuntime {
	rt := &ProcessTaskRuntime{
		tasks: make(map[TaskID]Task),
	}

	rand.Seed(time.Now().UnixNano())
	randomInt := rand.Intn(500) + ProcessRuntimeTcpListenPortBase
	rt.listenPort = fmt.Sprintf("%d", randomInt)

	go rt.runTCPServer(rt.listenPort)

	return rt
}

func (p *ProcessTaskRuntime) Start() error {
	return nil
}

func (p *ProcessTaskRuntime) Stop() error {
	return nil
}

func (p *ProcessTaskRuntime) CreateTask(cfg *TaskConfig) (Task, error) {
	log.Debugf("Creating process task with name: %s", cfg.Name)

	if cfg.Image != "" {
		return nil, fmt.Errorf("image not supported for process task")
	}

	task := NewProcessTask(cfg)

	log.Infof("Command: %s %v", cfg.Cmd, cfg.Args)

	// execute the task
	cmd := exec.Command(cfg.Cmd, cfg.Args...)
	cmd.Env = os.Environ()
	cmd.Env = append(cmd.Env, fmt.Sprintf("SERVICE_ADDR=127.0.0.1:%s", p.listenPort))
	cmd.Env = append(cmd.Env, fmt.Sprintf("SECRET=%d", task.secret))

	task.cmd = cmd

	p.tasks[task.ID()] = task
	return task, nil
}

func (p *ProcessTaskRuntime) runTCPServer(port string) {
	log.Infof("Starting docker hostcall TCP server on port %s", port)

	listener, err := net.Listen("tcp", fmt.Sprintf("0.0.0.0:%s", port))
	if err != nil {
		log.Fatal(err)
	}
	defer listener.Close()

	for {
		conn, err := listener.Accept()
		if err != nil {
			log.Fatal(err)
		}
		log.Debugf("Accepted connection from %s", conn.RemoteAddr())

		go p.handleRequest(conn)
	}
}

func (p *ProcessTaskRuntime) handleRequest(conn net.Conn) {
	// read a int64 secret from the connection (8 bytes, little endian)
	buf := make([]byte, 8)
	_, err := conn.Read(buf)
	if err != nil {
		log.Errorf("Error reading from initial connection: %v", err)
		return
	}
	secret := binary.LittleEndian.Uint64(buf)

	// find out the task
	for _, task := range p.tasks {
		if task.(*ProcessTask).secret == int64(secret) {
			// found the task
			task.(*ProcessTask).conn = conn
			task.(*ProcessTask).connReady <- struct{}{}
		}
	}
}
