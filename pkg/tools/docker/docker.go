package docker

import (
	"context"
	"fmt"
	"io"
	"os"
	"time"

	"bytes"

	"github.com/lfedgeai/spear/worker"

	"github.com/docker/docker/api/types/container"
	"github.com/docker/docker/api/types/image"
	"github.com/docker/docker/client"
	"github.com/docker/go-connections/nat"
)

type TestSetup struct {
	w        *worker.Worker
	vecStore *container.CreateResponse
}

func (t *TestSetup) stopVectorStoreContainer() {
	// stop the vector store container
	cli, err := client.NewClientWithOpts(client.FromEnv)
	if err != nil {
		panic(err)
	}

	err = cli.ContainerStop(context.TODO(), t.vecStore.ID, container.StopOptions{})
	if err != nil {
		panic(err)
	}

	err = cli.ContainerRemove(context.TODO(), t.vecStore.ID, container.RemoveOptions{})
	if err != nil {
		panic(err)
	}
}

func (t *TestSetup) startVectorStoreContainer() {
	// start the vector store container
	// docker run -p 6333:6333 -p 6334:6334 \
	// -v $(pwd)/qdrant_storage:/qdrant/storage:z \
	// qdrant/qdrant

	// start docker client
	cli, err := client.NewClientWithOpts(client.FromEnv)
	if err != nil {
		panic(err)
	}

	// pull the image
	r, err := cli.ImagePull(context.TODO(), "docker.io/qdrant/qdrant", image.PullOptions{})
	if err != nil {
		panic(err)
	}

	// read the response
	buf := new(bytes.Buffer)
	_, err = io.Copy(buf, r)
	if err != nil {
		panic(err)
	}

	// generate a random name for the container
	name := fmt.Sprintf("qdrant-%d", time.Now().Unix())

	// create the container
	c, err := cli.ContainerCreate(context.TODO(), &container.Config{
		Image: "qdrant/qdrant",
	}, &container.HostConfig{
		PortBindings: nat.PortMap{
			"6333/tcp": []nat.PortBinding{
				{
					HostIP:   "localhost",
					HostPort: "6333",
				},
			},
			"6334/tcp": []nat.PortBinding{
				{
					HostIP:   "localhost",
					HostPort: "6334",
				},
			},
		},
	}, nil, nil, name)
	if err != nil {
		panic(err)
	}

	// start the container
	err = cli.ContainerStart(context.TODO(), c.ID, container.StartOptions{})
	if err != nil {
		panic(err)
	}

	// wait for the container to start
	time.Sleep(5 * time.Second)

	// check the container status
	info, err := cli.ContainerInspect(context.TODO(), c.ID)
	if err != nil {
		panic(err)
	}

	if !info.State.Running {
		panic("container not running")
	}

	t.vecStore = &c
}

func NewTestSetup() *TestSetup {
	// check OPENAI_API_KEY environment variable
	if os.Getenv("OPENAI_API_KEY") == "" {
		panic("OPENAI_API_KEY environment variable not set")
	}

	t := &TestSetup{}

	t.startVectorStoreContainer()

	// setup the test environment
	cfg := worker.NewWorkerConfig("localhost", "8080", []string{}, true)
	t.w = worker.NewWorker(cfg)
	t.w.Init()
	go t.w.Run()
	time.Sleep(5 * time.Second)
	return t
}

func (t *TestSetup) TearDown() {
	// teardown the test environment
	t.w.Stop()

	t.stopVectorStoreContainer()
}
