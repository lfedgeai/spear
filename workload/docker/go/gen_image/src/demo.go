package main

import (
	"bufio"
	"bytes"
	"encoding/base64"
	"fmt"
	"net/http"
	"os"
	"os/exec"
	"runtime"

	"github.com/lfedgeai/spear/pkg/rpc/payload/transform"
	"github.com/lfedgeai/spear/pkg/tools/docker"
	log "github.com/sirupsen/logrus"
)

func init() {
	log.SetLevel(log.DebugLevel)
}

func main() {
	// get input from user
	reader := bufio.NewReader(os.Stdin)
	fmt.Print("Image Description: ")

	input, err := reader.ReadString('\n')
	if err != nil {
		panic("reader.ReadString failed: " + err.Error())
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
		panic("http.NewRequest failed: " + err.Error())
	}

	// add headers
	req.Header.Add("Content-Type", "application/json")
	req.Header.Add("Accept", "application/json")
	req.Header.Add("Spear-Func-Id", "3")
	req.Header.Add("Spear-Func-Type", "1")

	// send the request
	resp, err := client.Do(req)
	if err != nil {
		panic("client.Do failed: " + err.Error())
	}

	// print the response
	buf := new(bytes.Buffer)
	buf.ReadFrom(resp.Body)

	respData := buf.Bytes()
	log.Debugf("Received response length: %d", len(respData))

	var respStruct transform.ImageGenerationResponse
	err = respStruct.Unmarshal(respData)
	if err != nil {
		panic("respStruct.Unmarshal failed: " + err.Error())
	}

	if len(respStruct.Data) != 1 {
		panic("expected 1 image, got " + string(len(respStruct.Data)))
	}

	// resp is a image in base64 format
	// decode the image
	img := make([]byte, base64.StdEncoding.DecodedLen(len(respStruct.Data[0].B64Json)))
	_, err = base64.StdEncoding.Decode(img, []byte(respStruct.Data[0].B64Json))
	if err != nil {
		panic("base64.StdEncoding.Decode failed: " + err.Error())
	}

	// write the image to a temp file using os.CreateTemp
	file, err := os.CreateTemp("", "image-*.png")
	if err != nil {
		panic("os.CreateTemp failed: " + err.Error())
	}
	// write the image to the file
	_, err = file.Write(img)
	if err != nil {
		panic("file.Write failed: " + err.Error())
	}
	// close the file
	file.Close()

	// open the file using the default application
	err = openImage(file.Name())
	if err != nil {
		panic("openImage failed: " + err.Error())
	}

	// close the response body
	resp.Body.Close()
}

func openImage(filePath string) error {
	var cmd *exec.Cmd

	// Determine the command based on the OS
	switch runtime.GOOS {
	case "linux":
		cmd = exec.Command("xdg-open", filePath)
	case "darwin":
		cmd = exec.Command("open", filePath)
	case "windows":
		cmd = exec.Command("rundll32", "url.dll,FileProtocolHandler", filePath)
	default:
		return fmt.Errorf("unsupported platform")
	}

	return cmd.Start()
}
