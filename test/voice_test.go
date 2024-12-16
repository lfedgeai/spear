package test

import (
	"fmt"
	"os"
	"testing"
	"time"

	"github.com/lfedgeai/spear/pkg/io"
)

// Example callback to process audio data
func processAudio(buffer []byte, err error) {
	fmt.Println("Audio buffer:", buffer[:10]) // Print the first 10 samples as an example
}

func TestVoice(t *testing.T) {
	// Start recording audio and get the stop channel
	stopChan, err := io.RecordAudio(44100, processAudio)
	if err != nil {
		fmt.Fprintf(os.Stderr, "Failed to record audio: %v\n", err)
		os.Exit(1)
	}

	time.Sleep(5 * time.Second)

	// Signal to stop the recording
	close(stopChan)
}
