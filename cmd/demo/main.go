package main

import (
	"bufio"
	"bytes"
	"fmt"
	"net/http"
	"os"

	"github.com/lfedgeai/spear/pkg/tools/docker"
)

func main() {
	// get input from user
	reader := bufio.NewReader(os.Stdin)
	fmt.Print("Message to LLM: ")

	input, err := reader.ReadString('\n')
	if err != nil {
		fmt.Printf("Error: %v\n", err)
	}

	// setup test environment
	s := docker.NewTestSetup()
	defer s.TearDown()

	// send a http request to the server and check the response
	client := &http.Client{}
	req, err := http.NewRequest("GET", "http://localhost:8080", bytes.NewBuffer(
		[]byte(input),
	))

	if err != nil {
		fmt.Printf("Error: %v\n", err)
	}

	// add headers
	req.Header.Add("Content-Type", "application/json")
	req.Header.Add("Accept", "application/json")
	req.Header.Add("Spear-Func-Id", "2")
	req.Header.Add("Spear-Func-Type", "1")

	// send the request
	resp, err := client.Do(req)
	if err != nil {
		fmt.Printf("Error: %v\n", err)
	}

	// print the response
	buf := new(bytes.Buffer)
	buf.ReadFrom(resp.Body)

	// convert the response to map
	fmt.Println(buf.String())

	// close the response body
	resp.Body.Close()
}
