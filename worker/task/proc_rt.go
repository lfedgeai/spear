package task

import (
	"os/exec"

	log "github.com/sirupsen/logrus"
)

// implement TaskRuntimeProcess
type ProcessTaskRuntime struct {
}

func (p *ProcessTaskRuntime) CreateTask(cfg *TaskConfig) (Task, error) {
	log.Debugf("Creating process task with name: %s", cfg.Name)

	task := NewProcessTask(cfg)

	// execute the task
	cmd := exec.Command(cfg.Cmd, cfg.Args...)

	stdin, err := cmd.StdinPipe()
	if err != nil {
		return nil, err
	}
	stdout, err := cmd.StdoutPipe()
	if err != nil {
		return nil, err
	}

	task.cmd = cmd

	// create read goroutine to read from stdout and put into the channel
	go func() {
		defer close(task.out)
		for {
			buf := make([]byte, maxDataSize)
			n, err := stdout.Read(buf)
			if err != nil {
				if err.Error() == "EOF" {
					break
				}
				log.Errorf("Error reading stdout: %v", err)
				break
			}
			// log.Debugf("Output: %s", buf[:n])
			task.out <- buf[:n]

			// check if the task is done
			select {
			case <-task.done:
				log.Debugf("closing stdout")
				return
			default:
			}
		}
	}()

	// create write goroutine to write to stdin
	go func() {
		defer stdin.Close()
		for {
			select {
			case buf := <-task.in:
				stdin.Write(buf)
				// log.Debugf("Input: %s", buf)
			case <-task.done:
				log.Debugf("closing stdin")
				return
			}
		}
	}()

	// read from stderr and print to log
	stderr, err := cmd.StderrPipe()
	if err != nil {
		return nil, err
	}
	go func() {
		for {
			buf := make([]byte, maxDataSize)
			n, err := stderr.Read(buf)
			if err != nil {
				if err.Error() == "EOF" {
					break
				}
				log.Errorf("Error reading stderr: %v", err)
				break
			}
			log.Infof("[%s]stderr: %s", task.name, buf[:n])
		}
	}()

	return task, nil
}
