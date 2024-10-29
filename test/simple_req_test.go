package test

import (
	"net/http"
	"os"
	"testing"
	"time"

	"bytes"

	"github.com/lfedgeai/spear/worker"
)

var w *worker.Worker

func setupTest(t *testing.T) {
	// check OPENAI_API_KEY environment variable
	if os.Getenv("OPENAI_API_KEY") == "" {
		t.Fatalf("OPENAI_API_KEY environment variable not set")
	}
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
}

func TestSimpleReq(t *testing.T) {
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
