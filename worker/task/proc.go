package task

import (
	"fmt"
	"os/exec"

	log "github.com/sirupsen/logrus"
)

type ProcessTask struct {
	name string

	in  chan Message
	out chan Message

	cmd *exec.Cmd

	status TaskStatus

	result *error
	// a channel for the termination signal
	done chan struct{}
}

func (p *ProcessTask) ID() TaskID {
	return TaskID(p.cmd.Process.Pid)
}

func (p *ProcessTask) Start() error {
	p.cmd.Start()

	p.status = TaskStatusRunning

	go func() {
		if err := p.cmd.Wait(); err != nil {
			log.Errorf("Error: %v", err)
		}

		// set status to stopped
		p.status = TaskStatusStopped

		// close the done channel
		close(p.done)
	}()

	return nil
}

func (p *ProcessTask) Stop() error {
	// kill process
	if p.cmd.Process != nil {
		if err := p.cmd.Process.Kill(); err != nil {
			log.Errorf("Error: %v", err)
			return err
		}
		p.status = TaskStatusStopped
		return nil
	}
	return fmt.Errorf("process not started")
}

func (p *ProcessTask) Name() string {
	return p.name
}

func (p *ProcessTask) Status() TaskStatus {
	return p.status
}

func (p *ProcessTask) GetResult() *error {
	return p.result
}

func (p *ProcessTask) CommChannels() (chan Message, chan Message, error) {
	return p.in, p.out, nil
}

func (p *ProcessTask) Wait() (int, error) {
	<-p.done
	return 0, nil
}

func NewProcessTask(cfg *TaskConfig) *ProcessTask {
	return &ProcessTask{
		name:   cfg.Name,
		in:     make(chan Message, 1024),
		out:    make(chan Message, 1024),
		status: TaskStatusInit,
		result: nil,
		done:   make(chan struct{}),
	}
}
