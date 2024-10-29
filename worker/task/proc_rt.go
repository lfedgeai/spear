package task

import (
	"bufio"
	"fmt"
	"os"
	"os/exec"
	"syscall"

	log "github.com/sirupsen/logrus"
)

// implement TaskRuntimeProcess
type ProcessTaskRuntime struct {
}

func NewProcessTaskRuntime() *ProcessTaskRuntime {
	return &ProcessTaskRuntime{}
}

func (p *ProcessTaskRuntime) CreateTask(cfg *TaskConfig) (Task, error) {
	log.Debugf("Creating process task with name: %s", cfg.Name)

	task := NewProcessTask(cfg)

	// make sure there is no -o or -i in the args
	for _, arg := range cfg.Args {
		if arg == "-o" || arg == "-i" {
			return nil, fmt.Errorf("-i/-o is not allowed in the args")
		}
	}

	// create named pipe and add -i -o to the args
	// generate name for the named pipes
	inPipe := fmt.Sprintf("/tmp/%s_in_%d", cfg.Name, os.Getpid())
	outPipe := fmt.Sprintf("/tmp/%s_out_%d", cfg.Name, os.Getpid())

	// create the named pipes using os.mkfifo
	if err := syscall.Mkfifo(inPipe, 0666); err != nil {
		return nil, err
	}
	if err := syscall.Mkfifo(outPipe, 0666); err != nil {
		return nil, err
	}
	// add -i and -o to the args
	cfg.Args = append(cfg.Args, "-i", inPipe, "-o", outPipe)
	log.Infof("Command: %s %v", cfg.Cmd, cfg.Args)

	// execute the task
	cmd := exec.Command(cfg.Cmd, cfg.Args...)

	// open the named pipes
	inPipeFile, err := os.OpenFile(inPipe, os.O_RDWR, os.ModeNamedPipe)
	if err != nil {
		return nil, err
	}
	outPipeFile, err := os.OpenFile(outPipe, os.O_RDWR, os.ModeNamedPipe)
	if err != nil {
		return nil, err
	}

	task.cmd = cmd

	// create read goroutine to read and put into the channel
	go func() {
		defer close(task.out)
		reader := bufio.NewReader(outPipeFile)
		for {
			data, err := reader.ReadBytes('\n')
			if err != nil {
				break
			}

			task.out <- data

			// check if the task is done
			select {
			case <-task.done:
				log.Debugf("closing stdout")
				return
			default:
			}
		}
	}()

	// create write goroutine to write
	go func() {
		defer inPipeFile.Close()
		for {
			select {
			case buf := <-task.in:
				inPipeFile.Write(buf)
				// log.Debugf("Input: %s", buf)
			case <-task.done:
				log.Debugf("closing stdin")
				return
			}
		}
	}()

	// read from stderr and print to log
	stdout, err := cmd.StdoutPipe()
	if err != nil {
		return nil, err
	}
	go func() {
		for {
			buf := make([]byte, maxDataSize)
			n, err := stdout.Read(buf)
			if err != nil {
				break
			}
			log.Infof("STDOUT[%s]:\033[0;32m%s\033[0m", task.name, buf[:n])
		}
	}()
	stderr, err := cmd.StderrPipe()
	if err != nil {
		return nil, err
	}
	go func() {
		for {
			buf := make([]byte, maxDataSize)
			n, err := stderr.Read(buf)
			if err != nil {
				break
			}
			log.Infof("STDERR[%s]:\033[0;32m%s\033[0m", task.name, buf[:n])
		}
	}()

	return task, nil
}
