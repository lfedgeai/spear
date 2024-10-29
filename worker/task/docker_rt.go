package task

import (
	"context"

	"github.com/docker/docker/api/types/container"
	"github.com/docker/docker/client"
	log "github.com/sirupsen/logrus"
)

type DockerTaskRuntime struct {
	cli   *client.Client
	tasks map[TaskID]Task
	rtCfg *TaskRuntimeConfig
}

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

	return res, nil
}

func (d *DockerTaskRuntime) CreateTask(cfg *TaskConfig) (Task, error) {
	log.Infof("Creating docker task with name: %s", cfg.Name)

	containerCfg := &container.Config{
		Image: cfg.Image,
		// combine cfg.Cmd and cfg.Args
		Cmd:          append([]string{cfg.Cmd}, cfg.Args...),
		Tty:          false,
		AttachStdin:  true,
		AttachStdout: true,
		AttachStderr: true,
		OpenStdin:    true,
	}
	container, err := d.cli.ContainerCreate(context.TODO(), containerCfg,
		&container.HostConfig{
			AutoRemove: d.rtCfg.Debug,
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
	}

	// store the task
	d.tasks[TaskID(container.ID)] = res
	return res, nil
}
