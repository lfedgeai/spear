package test

import (
	"context"
	"io"
	"net/http"
	"os"
	"testing"
	"time"

	"bytes"

	"github.com/lfedgeai/spear/worker"

	"github.com/docker/docker/api/types/container"
	"github.com/docker/docker/api/types/image"
	"github.com/docker/docker/client"
	"github.com/docker/go-connections/nat"
)

var w *worker.Worker
var vecStore *container.CreateResponse

func stopVectorStoreContainer() {
	// stop the vector store container
	cli, err := client.NewClientWithOpts(client.FromEnv)
	if err != nil {
		panic(err)
	}

	err = cli.ContainerStop(context.TODO(), vecStore.ID, container.StopOptions{})
	if err != nil {
		panic(err)
	}

	err = cli.ContainerRemove(context.TODO(), vecStore.ID, container.RemoveOptions{})
	if err != nil {
		panic(err)
	}
}

func startVectorStoreContainer() {
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
	}, nil, nil, "qdrant")
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

	vecStore = &c
}

func setupTest(t *testing.T) {
	// check OPENAI_API_KEY environment variable
	if os.Getenv("OPENAI_API_KEY") == "" {
		t.Fatalf("OPENAI_API_KEY environment variable not set")
	}

	startVectorStoreContainer()

	// setup the test environment
	cfg := worker.NewWorkerConfig("localhost", "8080", []string{}, true)
	w = worker.NewWorker(cfg)
	w.Init()
	go w.Run()
	time.Sleep(5 * time.Second)
}

func teardownTest(_ *testing.T) {
	// teardown the test environment
	w.Stop()

	stopVectorStoreContainer()
}

func TestSimpleReq(t *testing.T) {
	// TestSimpleReq tests simple requests to the worker
	// ┌──────────────────┐
	// │                  │
	// │                  │
	// │      Docker      │
	// │   Vector Store   │
	// │                  │
	// └───────────┬──────┘
	//        ▲    │
	//        │    ▼
	// ┌──────┴───────────┐
	// │                  │
	// │                  │
	// │      Worker      │
	// │                  │
	// │                  │
	// └────────────┬─────┘
	//       ▲      │
	//       │      ▼
	// ┌─────┴────────────┐
	// │                  │
	// │                  │
	// │      Task        │
	// │                  │
	// │                  │
	// └──────────────────┘

	// setup the test environment
	setupTest(t)
	defer teardownTest(t)
	// send a http request to the server and check the response

	// create a http client
	client := &http.Client{}

	// create a http request
	req, err := http.NewRequest("GET", "http://localhost:8080", bytes.NewBuffer(
		[]byte(
			`this is a
			multiline test`,
		),
	))
	if err != nil {
		t.Fatalf("Error: %v", err)
	}

	// add headers
	req.Header.Add("Content-Type", "application/json")
	req.Header.Add("Accept", "application/json")
	req.Header.Add("Spear-Func-Id", "1")
	req.Header.Add("Spear-Func-Type", "1")

	// send the request
	resp, err := client.Do(req)
	if err != nil {
		t.Fatalf("Error: %v", err)
	}

	// check the response
	if resp.StatusCode != http.StatusOK {
		msg := make([]byte, 1024)
		n, _ := resp.Body.Read(msg)
		t.Fatalf("Error: %v %s", resp.Status, msg[:n])
	}

	// close the response body
	defer resp.Body.Close()
}
