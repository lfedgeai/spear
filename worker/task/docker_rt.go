package task

import (
	"context"
	"encoding/binary"
	"fmt"
	"math/rand"
	"net"
	"time"

	"github.com/docker/docker/api/types/container"
	"github.com/docker/docker/client"
	log "github.com/sirupsen/logrus"
)

type DockerTaskRuntime struct {
	cli   *client.Client
	tasks map[TaskID]Task
	rtCfg *TaskRuntimeConfig
}

var DockerRuntimeTcpListenPort = "8383"

func NewDockerTaskRuntime(rtCfg *TaskRuntimeConfig) (*DockerTaskRuntime, error) {
	// create docker client
	cli, err := client.NewClientWithOpts(client.FromEnv)
	if err != nil {
		return nil, err
	}

	res := &DockerTaskRuntime{
		cli:   cli,
		tasks: make(map[TaskID]Task),
		rtCfg: rtCfg,
	}

	go res.runTCPServer(DockerRuntimeTcpListenPort)

	return res, nil
}

func (d *DockerTaskRuntime) CreateTask(cfg *TaskConfig) (Task, error) {
	log.Debugf("Creating docker task [%s]", cfg.Name)

	rand.Seed(time.Now().UnixNano())
	secretGenerated := rand.Int63()

	args := append([]string{cfg.Cmd}, cfg.Args...)
	args = append(args, "--service-addr", fmt.Sprintf("host.docker.internal:%s", DockerRuntimeTcpListenPort))
	args = append(args, "--secret", fmt.Sprintf("%d", secretGenerated))
	containerCfg := &container.Config{
		Image: cfg.Image,
		// combine cfg.Cmd and cfg.Args
		Cmd:          args,
		Tty:          false,
		AttachStdin:  true,
		AttachStdout: true,
		AttachStderr: true,
		OpenStdin:    true,
	}
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
	}

	// store the task
	d.tasks[TaskID(container.ID)] = res
	return res, nil
}

func (d *DockerTaskRuntime) runTCPServer(port string) {
	log.Debugf("Starting docker hostcall TCP server on port %s", port)

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
