package task

import (
	"context"
	"encoding/binary"
	"fmt"
	"io"
	"net"

	"github.com/docker/docker/api/types"
	"github.com/docker/docker/api/types/container"
	log "github.com/sirupsen/logrus"
)

type DockerTask struct {
	name      string
	container *container.CreateResponse
	runtime   *DockerTaskRuntime

	attachResp *types.HijackedResponse
	chanIn     chan Message
	chanOut    chan Message

	secret    int64 // used for tcp auth
	conn      net.Conn
	connReady chan struct{}

	reqId uint64
}

func (p *DockerTask) ID() TaskID {
	return TaskID(p.container.ID)
}

func (p *DockerTask) Start() error {
	err := p.runtime.cli.ContainerStart(context.TODO(), p.container.ID, container.StartOptions{})
	if err != nil {
		return err
	}

	go func() {
		<-p.connReady
		log.Debugf("Connection ready for task %s", p.name)

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

	// get stdin and stdout
	val, err := p.runtime.cli.ContainerAttach(context.TODO(), p.container.ID, container.AttachOptions{
		Stream: true,
		Stdin:  true,
		Stdout: false,
		Stderr: false,
	})
	if err != nil {
		return err
	}
	p.attachResp = &val

	resp, err := p.runtime.cli.ContainerLogs(context.TODO(),
		p.container.ID, container.LogsOptions{
			ShowStdout: true,
			ShowStderr: true,
			Follow:     true,
			Timestamps: false,
		})
	if err != nil {
		return err
	}

	go func() {
		for {
			data := make([]byte, SPEARDockerReadBufferSize)
			n, err := resp.Read(data)
			if err != nil {
				continue
			}

			if n == 0 {
				log.Debugf("no data")
				break
			}
			if n <= 8 {
				log.Errorf("invalid data: %s", data[:n])
				continue
			}
			for {
				// loop through data and send messages
				header := data[:8]
				data = data[8:]
				n = n - 8
				// big endian size
				sz := int(header[4])<<24 | int(header[5])<<16 | int(header[6])<<8 | int(header[7])
				log.Debugf("Size: %d, ReadLen: %d, Got data: %s", sz, n, data)
				if header[0] == 0x01 {
					// stdout
					data2print := data[:sz]
					// remove trailing newline
					if data2print[sz-1] == '\n' {
						data2print = data2print[:sz-1]
					}
					log.Infof("STDOUT[%s]:\033[0;36m%s\033[0m", p.name, data2print)
				} else if header[0] == 0x02 {
					// stderr
					data2print := data[:sz]
					// remove trailing newline
					if data2print[sz-1] == '\n' {
						data2print = data2print[:sz-1]
					}
					log.Infof("STDERR[%s]:\033[0;31m%s\033[0m", p.name, data2print)
				}
				if sz >= n || sz == 0 {
					break
				}
				data = data[sz:]
			}
		}
	}()
	return nil
}

func (p *DockerTask) Stop() error {
	err := p.runtime.cli.ContainerStop(context.TODO(), p.container.ID, container.StopOptions{})
	if err != nil {
		return err
	}
	return nil
}

func (p *DockerTask) Name() string {
	return p.name
}

func (p *DockerTask) Status() TaskStatus {
	return TaskStatusRunning
}

func (p *DockerTask) GetResult() *error {
	err := fmt.Errorf("not implemented")
	return &err
}

func (p *DockerTask) CommChannels() (chan Message, chan Message, error) {
	return p.chanIn, p.chanOut, nil
}

func (p *DockerTask) Wait() (int, error) {
	c, err := p.runtime.cli.ContainerWait(context.TODO(), p.container.ID, container.WaitConditionNotRunning)
	select {
	case <-c:
		return 0, nil

	case e := <-err:
		return -1, e
	}
}

func (p *DockerTask) NextRequestID() uint64 {
	p.reqId++
	return p.reqId
}
