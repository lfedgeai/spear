package task

import (
	"context"
	"encoding/binary"
	"fmt"
	"math/rand"
	"net"
	"sync"
	"time"

	"github.com/docker/docker/api/types/container"
	"github.com/docker/docker/client"
	"github.com/lfedgeai/spear/worker/task/docker"
	log "github.com/sirupsen/logrus"
)

type DockerTaskRuntime struct {
	cli        *client.Client
	tasks      map[TaskID]Task
	rtCfg      *TaskRuntimeConfig
	containers map[string]*container.CreateResponse
	stopCh     chan struct{}
	stopWg     sync.WaitGroup
}

const (
	DockerRuntimeTcpListenPortBase = 8100
)

var (
	DockerRuntimeTcpListenPort = ""
)

func NewDockerTaskRuntime(rtCfg *TaskRuntimeConfig) (*DockerTaskRuntime, error) {
	// create docker client
	cli, err := client.NewClientWithOpts(client.FromEnv)
	if err != nil {
		return nil, err
	}

	// generate a random port to use
	rand.Seed(time.Now().UnixNano())
	randomInt := rand.Intn(500) + DockerRuntimeTcpListenPortBase
	DockerRuntimeTcpListenPort = fmt.Sprintf("%d", randomInt)

	res := &DockerTaskRuntime{
		cli:        cli,
		tasks:      make(map[TaskID]Task),
		rtCfg:      rtCfg,
		containers: make(map[string]*container.CreateResponse),
		stopCh:     make(chan struct{}),
		stopWg:     sync.WaitGroup{},
	}

	go res.runTCPServer(DockerRuntimeTcpListenPort)

	res.stopWg.Add(1)
	go func() {
		defer res.stopWg.Done()
		<-res.stopCh
		log.Debugf("Stopping docker task runtime")
		for _, task := range res.containers {
			if err := docker.StopContainer(task.ID); err != nil {
				log.Errorf("Error stopping container %s: %v", task.ID, err)
			}
		}
	}()

	if rtCfg.StartServices {
		if err := res.startBackendServices(); err != nil {
			return nil, err
		}
	}

	return res, nil
}

func (d *DockerTaskRuntime) Start() error {
	return nil
}

func (d *DockerTaskRuntime) Stop() error {
	d.stopCh <- struct{}{}
	// iterate all tasks and stop them
	for _, task := range d.tasks {
		if err := task.Stop(); err != nil {
			log.Errorf("Error stopping task %s: %v", task.ID(), err)
		}
	}
	d.stopWg.Wait()
	return nil
}

func (d *DockerTaskRuntime) startBackendServices() error {
	// start the vector store container
	// docker run -p 6333:6333 -p 6334:6334 \
	// -v $(pwd)/qdrant_storage:/qdrant/storage:z \
	// qdrant/qdrant

	c, err := docker.StartVectorStoreContainer(d.rtCfg.Cleanup)
	if err != nil {
		return err
	}
	d.containers["vector_store"] = c
	return nil
}

func (d *DockerTaskRuntime) CreateTask(cfg *TaskConfig) (Task, error) {
	log.Debugf("Creating docker task [%s]", cfg.Name)

	rand.Seed(time.Now().UnixNano())
	secretGenerated := rand.Int63()

	args := append([]string{cfg.Cmd}, cfg.Args...)
	containerCfg := &container.Config{
		Image: cfg.Image,
		// combine cfg.Cmd and cfg.Args
		Cmd:          args,
		Tty:          false,
		AttachStdin:  true,
		AttachStdout: true,
		AttachStderr: true,
		OpenStdin:    true,
		Env: []string{
			fmt.Sprintf("SERVICE_ADDR=host.docker.internal:%s", DockerRuntimeTcpListenPort),
			fmt.Sprintf("SECRET=%d", secretGenerated),
		},
	}
	log.Debugf("Creating container with env: %v", containerCfg.Env)
	container, err := d.cli.ContainerCreate(context.TODO(), containerCfg,
		&container.HostConfig{
			AutoRemove: !d.rtCfg.Debug,
		},
		nil, nil, cfg.Name)
	if err != nil {
		return nil, err
	}

	res := &DockerTask{
		name:      cfg.Name,
		container: &container,
		runtime:   d,

		attachResp: nil,
		chanIn:     make(chan Message, 100),
		chanOut:    make(chan Message, 100),

		secret:    secretGenerated,
		conn:      nil,
		connReady: make(chan struct{}),

		reqId: 0,

		taskVars:   make(map[TaskVar]interface{}),
		taskVarsMu: sync.RWMutex{},
	}

	// store the task
	d.tasks[TaskID(container.ID)] = res
	return res, nil
}

func (d *DockerTaskRuntime) runTCPServer(port string) {
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

		go d.handleRequest(conn)
	}
}

func (d *DockerTaskRuntime) handleRequest(conn net.Conn) {
	// read a int64 secret from the connection (8 bytes, little endian)
	buf := make([]byte, 8)
	_, err := conn.Read(buf)
	if err != nil {
		log.Errorf("Error reading from initial connection: %v", err)
		return
	}
	secret := binary.LittleEndian.Uint64(buf)

	// find out the task
	for _, task := range d.tasks {
		if task.(*DockerTask).secret == int64(secret) {
			// found the task
			task.(*DockerTask).conn = conn
			task.(*DockerTask).connReady <- struct{}{}
		}
	}
}
