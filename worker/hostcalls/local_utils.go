package hostcalls

import (
	"bufio"
	"encoding/base64"
	"encoding/json"
	"fmt"
	"os"
	"time"

	"github.com/faiface/beep"
	"github.com/faiface/beep/mp3"
	"github.com/faiface/beep/speaker"
	hostcalls "github.com/lfedgeai/spear/worker/hostcalls/common"
	"github.com/schollz/progressbar/v3"
	log "github.com/sirupsen/logrus"
)

func Input(inv *hostcalls.InvocationInfo, args interface{}) (interface{}, error) {
	// read from stdin
	fmt.Print(args.(string))
	reader := bufio.NewReader(os.Stdout)

	// Read a line from stdout
	line, err := reader.ReadString('\n')
	if err != nil {
		fmt.Println("Error reading from stdout:", err)
		return nil, err
	}

	return line, nil
}

func Speak(inv *hostcalls.InvocationInfo, args interface{}) (interface{}, error) {
	// speak the text
	// umarshal the response

	var data map[string]interface{}
	err := json.Unmarshal([]byte(args.(string)), &data)
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
		return nil, fmt.Errorf("os.CreateTemp failed: " + err.Error())
	}
	defer os.Remove(f.Name())

	log.Debugf("Data Length: %d", len(rawData))
	// wrtie the audio data to the file
	_, err = f.Write(rawData)
	if err != nil {
		return nil, fmt.Errorf("f.Write failed: " + err.Error())
	}
	f.Close()
	log.Debugf("Created temp file: %s", f.Name())

	err = playMP3(f.Name())
	if err != nil {
		return nil, fmt.Errorf("could not play MP3 file: %w", err)
	}

	return nil, nil
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

	// create progress bar
	bar := progressbar.NewOptions64(
		-1,
		progressbar.OptionSetDescription("Speaking..."),
		progressbar.OptionSetWriter(os.Stderr),
		progressbar.OptionSetWidth(10),
		progressbar.OptionOnCompletion(func() {
			fmt.Fprint(os.Stderr, "\n")
		}),
		progressbar.OptionSpinnerType(14),
		progressbar.OptionFullWidth(),
		progressbar.OptionSetRenderBlankState(true),
	)

	// Play the audio stream
	done := make(chan bool)
	speaker.Play(beep.Seq(stream, beep.Callback(func() {
		done <- true
	})))

	for {
		// update the progress bar
		bar.Add(stream.Position())
		// check if the audio is done playing
		select {
		case <-done:
			bar.Describe("Done")
			bar.Close()
			return nil
		default:
			time.Sleep(100 * time.Millisecond)
		}
	}
}
