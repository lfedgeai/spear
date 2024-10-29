package task

import (
	"context"
	"fmt"

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
}

func (p *DockerTask) ID() TaskID {
	return TaskID(p.container.ID)
}

func (p *DockerTask) Start() error {
	err := p.runtime.cli.ContainerStart(context.TODO(), p.container.ID, container.StartOptions{})
	if err != nil {
		return err
	}

	// get stdin and stdout
	val, err := p.runtime.cli.ContainerAttach(context.TODO(), p.container.ID, container.AttachOptions{
		Stream: false,
		Stdin:  true,
		Stdout: false,
		Stderr: false,
	})
	if err != nil {
		return err
	}
	p.attachResp = &val
	go func() {
		for msg := range p.chanIn {
			// log.Debugf("Got message for container: %s", msg)
			_, err := p.attachResp.Conn.Write(msg)
			if err != nil {
				log.Errorf("Error writing to container: %v", err)
			}
		}
	}()

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
				// log.Debugf("Size: %d, ReadLen: %d, Got data: %s", sz, n, data)
				if header[0] == 0x01 {
					// stdout
					p.chanOut <- Message(data[:sz])
				} else if header[0] == 0x02 {
					// stderr
					log.Infof("STDERR[%s]:\033[0;32m%s\033[0m", p.name, data[:sz])
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
