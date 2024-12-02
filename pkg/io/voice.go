package io

import (
	"bytes"
	"encoding/binary"
	"fmt"
	"sync"

	"github.com/gordonklaus/portaudio"
)

// RecordAudio starts recording audio and returns a channel to stop the recording.
// The callback is called once with all collected audio data after the recording stops.
func RecordAudio(sampleRate float64, callback func(data []byte, err error)) (chan struct{}, error) {
	// Initialize PortAudio
	portaudio.Initialize()

	// Create a buffer for audio data and a slice to store all data
	buffer := make([]int16, 1024) // Adjust buffer size as needed
	var audioData []int16
	var mu sync.Mutex

	// Open a stream for audio input
	stream, err := portaudio.OpenDefaultStream(1, 0, sampleRate, len(buffer), func(in []int16) {
		mu.Lock()
		audioData = append(audioData, in...)
		mu.Unlock()
	})
	if err != nil {
		portaudio.Terminate()
		return nil, fmt.Errorf("error opening audio stream: %w", err)
	}

	// Start the stream
	if err := stream.Start(); err != nil {
		portaudio.Terminate()
		return nil, fmt.Errorf("error starting audio stream: %w", err)
	}

	// Channel to stop recording
	stopChan := make(chan struct{})

	// Goroutine to handle recording
	go func() {
		<-stopChan

		// Stop and close the stream
		stream.Stop()
		stream.Close()
		portaudio.Terminate()

		// convert audio data to WAV format using ConvertRawToWAVData
		wavData, err := ConvertRawToWAVData(audioData, int(sampleRate), 1)
		if err != nil {
			callback(nil, fmt.Errorf("failed to convert audio data to WAV: %w", err))
			return
		}

		// Call the callback with the WAV data
		callback(wavData, nil)
	}()

	return stopChan, nil
}

// ConvertRawToWAVData converts raw audio data to WAV format and returns the WAV data.
func ConvertRawToWAVData(data []int16, sampleRate, channels int) ([]byte, error) {
	// Define WAV file header
	var wavHeader = struct {
		ChunkID       [4]byte // "RIFF"
		ChunkSize     uint32  // File size - 8 bytes
		Format        [4]byte // "WAVE"
		Subchunk1ID   [4]byte // "fmt "
		Subchunk1Size uint32  // PCM header size (16)
		AudioFormat   uint16  // PCM = 1
		NumChannels   uint16  // Mono = 1, Stereo = 2
		SampleRate    uint32
		ByteRate      uint32
		BlockAlign    uint16
		BitsPerSample uint16
		Subchunk2ID   [4]byte // "data"
		Subchunk2Size uint32  // Raw audio data size
	}{
		ChunkID:       [4]byte{'R', 'I', 'F', 'F'},
		Format:        [4]byte{'W', 'A', 'V', 'E'},
		Subchunk1ID:   [4]byte{'f', 'm', 't', ' '},
		Subchunk1Size: 16,
		AudioFormat:   1,
		NumChannels:   uint16(channels),
		SampleRate:    uint32(sampleRate),
		ByteRate:      uint32(sampleRate * channels * 2), // SampleRate * NumChannels * BitsPerSample/8
		BlockAlign:    uint16(channels * 2),              // NumChannels * BitsPerSample/8
		BitsPerSample: 16,
		Subchunk2ID:   [4]byte{'d', 'a', 't', 'a'},
		Subchunk2Size: uint32(len(data) * 2), // Data size in bytes
	}
	wavHeader.ChunkSize = 36 + wavHeader.Subchunk2Size

	// create a buffer to store the WAV data
	var buf bytes.Buffer
	// Write WAV header
	if err := binary.Write(&buf, binary.LittleEndian, wavHeader); err != nil {
		return nil, fmt.Errorf("failed to write WAV header: %w", err)
	}
	// Write raw audio data
	if err := binary.Write(&buf, binary.LittleEndian, data); err != nil {
		return nil, fmt.Errorf("failed to write WAV data: %w", err)
	}

	return buf.Bytes(), nil
}
