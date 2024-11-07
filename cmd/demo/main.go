package main

import (
	"bufio"
	"bytes"
	"encoding/base64"
	"encoding/json"
	"fmt"
	"net/http"
	"os"
	"time"

	"github.com/faiface/beep"
	"github.com/faiface/beep/mp3"
	"github.com/faiface/beep/speaker"
	"github.com/lfedgeai/spear/pkg/tools/docker"
	log "github.com/sirupsen/logrus"
)

func init() {
	log.SetLevel(log.DebugLevel)
}

func main() {
	// get input from user
	reader := bufio.NewReader(os.Stdin)
	fmt.Print("Message to LLM: ")

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
	req.Header.Add("Spear-Func-Id", "2")
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

	// umarshal the response
	var data map[string]interface{}
	err = json.Unmarshal([]byte(respData), &data)
	if err != nil {
		panic("json.Unmarshal failed: " + err.Error())
	}

	// get the "audio" key from the response
	encodedData, ok := data["audio"]
	if !ok {
		panic("audio key not found in response")
	}
	// convert from base64 to []byte
	rawData, err := base64.StdEncoding.DecodeString(encodedData.(string))
	if err != nil {
		panic("base64.StdEncoding.DecodeString failed: " + err.Error())
	}

	// write to a temp file
	f, err := os.CreateTemp("", "audio*.mp3")
	if err != nil {
		panic("os.CreateTemp failed: " + err.Error())
	}
	log.Debugf("Data Length: %d", len(rawData))
	// wrtie the audio data to the file
	_, err = f.Write(rawData)
	if err != nil {
		panic("f.Write failed: " + err.Error())
	}
	f.Close()
	log.Debugf("Created temp file: %s", f.Name())

	playMP3(f.Name())

	// close the response body
	resp.Body.Close()
}

func playMP3(filePath string) error {
	// Open the MP3 file
	f, err := os.Open(filePath)
	if err != nil {
		return fmt.Errorf("could not open MP3 file: %w", err)
	}
	defer f.Close()

	// Decode the MP3 file
	stream, format, err := mp3.Decode(f)
	if err != nil {
		return fmt.Errorf("could not decode MP3 file: %w", err)
	}
	defer stream.Close()

	// Initialize the speaker with the sample rate
	err = speaker.Init(format.SampleRate, format.SampleRate.N(time.Second/10))
	if err != nil {
		return fmt.Errorf("could not initialize speaker: %w", err)
	}

	// Play the audio stream
	done := make(chan bool)
	speaker.Play(beep.Seq(stream, beep.Callback(func() {
		done <- true
	})))

	// Wait until the audio finishes playing
	<-done
	return nil
}
