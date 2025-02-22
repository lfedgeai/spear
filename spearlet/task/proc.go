package task

import (
	"encoding/binary"
	"errors"
	"fmt"
	"io"
	"math/rand"
	"net"
	"os/exec"
	"strconv"
	"sync"
	"syscall"
	"time"

	log "github.com/sirupsen/logrus"
)

type ProcessTask struct {
	name   string
	taskid TaskID

	cmd *exec.Cmd

	status TaskStatus

	result *error
	// a channel for the termination signal
	done chan struct{}

	chanIn  chan Message
	chanOut chan Message

	secret    int64 // used for tcp auth
	conn      net.Conn
	connReady chan struct{}

	taskVars   map[TaskVar]interface{}
	taskVarsMu sync.RWMutex

	reqId uint64
}

func (p *ProcessTask) ID() TaskID {
	return p.taskid
}

func (p *ProcessTask) Start() error {
	log.Infof("running command: %+v", p.cmd)

	// read from stderr and print to log
	stdout, err := p.cmd.StdoutPipe()
	if err != nil {
		return err
	}
	go func() {
		for {
			buf := make([]byte, maxDataSize)
			n, err := stdout.Read(buf)
			if err != nil {
				break
			}
			log.Infof("STDOUT[%s]:\033[0;32m%s\033[0m", p.name, buf[:n])
		}
	}()
	stderr, err := p.cmd.StderrPipe()
	if err != nil {
		return err
	}
	go func() {
		for {
			buf := make([]byte, maxDataSize)
			n, err := stderr.Read(buf)
			if err != nil {
				break
			}
			log.Infof("STDERR[%s]:\033[0;32m%s\033[0m", p.name, buf[:n])
		}
	}()

	err = p.cmd.Start()
	if err != nil {
		log.Errorf("Error: %v", err)
		return err
	}

	p.status = TaskStatusRunning

	go func() {
		if err := p.cmd.Wait(); err != nil {
			// get stderr output
			log.Infof("Wait error. %v, command %s", err, p.cmd.String())
		}

		// set status to stopped
		p.status = TaskStatusStopped

		// close the done channel
		close(p.done)
	}()

	go func() {
		<-p.connReady
		log.Infof("Connection ready for task %s", p.name)

		// input goroutine
		go func() {
			for {
				// read a int64 data size
				buf := make([]byte, 8)
				_, err := p.conn.Read(buf)
				if err != nil {
					if err == io.EOF {
						log.Infof("Connection closed for task %s", p.name)
						return
					}
					if errors.Is(err, syscall.ECONNRESET) {
						log.Warnf("Connection reset for task %s", p.name)
						return
					}
					log.Errorf("Error reading from connection: %v", err)
					return
				}
				sz := binary.LittleEndian.Uint64(buf)
				log.Debugf("DockerTask got message size: 0x%x", sz)

				// read data
				data := make([]byte, sz)
				if _, err = io.ReadFull(p.conn, data); err != nil {
					log.Errorf("Error reading from connection: %v, size: %d", err, sz)
					return
				}

				// send data to container
				p.chanOut <- Message(data)
			}
		}()

		// output goroutine
		go func() {
			for msg := range p.chanIn {
				// write little endian int64 size
				buf := make([]byte, 8)
				binary.LittleEndian.PutUint64(buf, uint64(len(msg)))
				_, err := p.conn.Write(buf)
				if err != nil {
					log.Errorf("Error writing to connection: %v", err)
					return
				}

				// write data
				n, err := p.conn.Write([]byte(msg))
				if n != len(msg) {
					log.Errorf("Error writing to connection: %v", err)
					return
				}
				if err != nil {
					log.Errorf("Error writing to connection: %v", err)
					return
				}
			}
		}()
	}()

	return nil
}

func (p *ProcessTask) Stop() error {
	// kill process
	if p.cmd.Process != nil {
		if err := p.cmd.Process.Kill(); err != nil {
			log.Errorf("Error stopping task: %v", err)
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
	return p.chanIn, p.chanOut, nil
}

func (p *ProcessTask) Wait() (int, error) {
	<-p.done
	return 0, nil
}

func (p *ProcessTask) NextRequestID() uint64 {
	res := p.reqId
	p.reqId += 1
	return res
}

func (p *ProcessTask) SetVar(key TaskVar, value interface{}) {
	p.taskVarsMu.Lock()
	defer p.taskVarsMu.Unlock()
	if value == nil {
		delete(p.taskVars, key)
	}
	p.taskVars[key] = value
}

func (p *ProcessTask) GetVar(key TaskVar) (interface{}, bool) {
	p.taskVarsMu.RLock()
	defer p.taskVarsMu.RUnlock()
	if _, ok := p.taskVars[key]; !ok {
		return nil, false
	} else {
		return p.taskVars[key], true
	}
}

func NewProcessTask(cfg *TaskConfig) *ProcessTask {
	rand.Seed(time.Now().UnixNano())
	secretGenerated := rand.Int63()

	return &ProcessTask{
		name:   cfg.Name,
		taskid: TaskID(strconv.Itoa(rand.Int())),
		status: TaskStatusInit,
		result: nil,
		done:   make(chan struct{}),

		chanIn:  make(chan Message, 128),
		chanOut: make(chan Message, 128),

		secret:    secretGenerated,
		conn:      nil,
		connReady: make(chan struct{}),

		reqId: 1,

		taskVars:   make(map[TaskVar]interface{}),
		taskVarsMu: sync.RWMutex{},
	}
}
