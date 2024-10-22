package test

import (
	"net/http"
	"testing"

	"bytes"
)

func TestSimpleReq(t *testing.T) {
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
	req.Header.Add("Spear-Func-Id", "1234")

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
