package test

import (
	"fmt"
	"net/http"
	"testing"

	"bytes"

	"github.com/lfedgeai/spear/pkg/tools/docker"
	"github.com/lfedgeai/spear/worker"
	"github.com/lfedgeai/spear/worker/task"
)

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
	s := docker.NewTestSetup()
	defer s.TearDown()
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

	// print body
	msg := make([]byte, 1024)
	n, _ := resp.Body.Read(msg)
	fmt.Printf("Response: %s\n", msg[:n])

	// close the response body
	resp.Body.Close()
}

func TestLocalDummy(t *testing.T) {
	// create config
	config := worker.NewExecWorkerConfig(true)
	w := worker.NewWorker(config)
	w.Initialize()

	res, err := w.ExecuteTask(1, task.TaskTypeDocker, true, "handle", "")
	if err != nil {
		t.Fatalf("Error executing workload: %v", err)
	}
	t.Logf("Workload execution result: %v", res)
	w.Stop()
}

func TestLocalPydummy(t *testing.T) {
	// create config
	config := worker.NewExecWorkerConfig(true)
	w := worker.NewWorker(config)
	w.Initialize()

	res, err := w.ExecuteTask(7, task.TaskTypeDocker, true, "handle", "")
	if err != nil {
		t.Fatalf("Error executing workload: %v", err)
	}
	t.Logf("Workload execution result: %v", res)
	w.Stop()
}
