package hostcalls

import (
	"bufio"
	"encoding/base64"
	"fmt"
	"os"
	"sync"
	"time"

	flatbuffers "github.com/google/flatbuffers/go"
	"github.com/lfedgeai/spear/pkg/io"
	protoio "github.com/lfedgeai/spear/pkg/spear/proto/io"

	"github.com/faiface/beep"
	"github.com/faiface/beep/mp3"
	"github.com/faiface/beep/speaker"
	hostcalls "github.com/lfedgeai/spear/spearlet/hostcalls/common"
	"github.com/schollz/progressbar/v3"

	log "github.com/sirupsen/logrus"
)

const (
	defaultTTSModel  = "tts-1"
	defaultTTSVoice  = "nova"
	defaultTTSFormat = "mp3"
	defaultSTTModel  = "whisper-1"
)

func Input(inv *hostcalls.InvocationInfo, args []byte) ([]byte, error) {
	req := protoio.GetRootAsInputRequest(args, 0)
	if req == nil {
		return nil, fmt.Errorf("could not get InputRequest")
	}

	// display the prompt
	fmt.Print(req.Prompt())
	reader := bufio.NewReader(os.Stdout)

	// Read a line from stdout
	line := ""
	var err error
	if req.Dryrun() {
		line = "test"
	} else {
		line, err = reader.ReadString('\n')
		if err != nil {
			fmt.Println("Error reading from stdout:", err)
			return nil, err
		}
	}

	builder := flatbuffers.NewBuilder(0)
	lineOff := builder.CreateString(line)

	protoio.InputResponseStart(builder)
	protoio.InputResponseAddText(builder, lineOff)
	builder.Finish(protoio.InputResponseEnd(builder))

	return builder.FinishedBytes(), nil
}

func Speak(inv *hostcalls.InvocationInfo, args []byte) ([]byte, error) {
	// speak the text
	req := protoio.GetRootAsSpeakRequest(args, 0)
	if req == nil {
		return nil, fmt.Errorf("could not get SpeakRequest")
	}

	transcript := req.Text()
	model := defaultTTSModel
	voice := defaultTTSVoice
	format := defaultTTSFormat

	if len(req.Model()) > 0 {
		model = string(req.Model())
	}
	if len(req.Voice()) > 0 {
		voice = string(req.Voice())
	}
	if len(req.Format()) > 0 {
		format = string(req.Format())
	}

	encodedData, err := textToSpeechData(string(transcript), model, voice, format)
	if err != nil {
		return nil, fmt.Errorf("error getting audio data: %w", err)
	}

	log.Debugf("Speaking: %s", transcript)

	// convert from base64 to []byte
	rawData, err := base64.StdEncoding.DecodeString(encodedData)
	if err != nil {
		panic("base64.StdEncoding.DecodeString failed: " + err.Error())
	}

	log.Debugf("Data: %v", rawData)

	// write to a temp file
	f, err := os.CreateTemp("", "audio*.mp3")
	if err != nil {
		return nil, fmt.Errorf("os.CreateTemp failed: %s", err.Error())
	}
	defer os.Remove(f.Name())

	log.Debugf("Data Length: %d", len(rawData))
	// wrtie the audio data to the file
	_, err = f.Write(rawData)
	if err != nil {
		return nil, fmt.Errorf("f.Write failed: %s", err.Error())
	}
	f.Close()
	log.Debugf("Created temp file: %s", f.Name())

	if req.Dryrun() {
		err = playMP3(f.Name())
		if err != nil {
			return nil, fmt.Errorf("could not play MP3 file: %w", err)
		}
	}

	builder := flatbuffers.NewBuilder(0)
	dataOff := builder.CreateString(encodedData)

	protoio.SpeakResponseStart(builder)
	protoio.SpeakResponseAddData(builder, dataOff)
	builder.Finish(protoio.SpeakResponseEnd(builder))

	return builder.FinishedBytes(), nil
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

func Record(inv *hostcalls.InvocationInfo, args []byte) ([]byte, error) {
	req := protoio.GetRootAsRecordRequest(args, 0)
	if req == nil {
		return nil, fmt.Errorf("could not get RecordRequest")
	}

	if req.Dryrun() {
		builder := flatbuffers.NewBuilder(0)
		textOff := builder.CreateString("test test test")

		protoio.RecordResponseStart(builder)
		protoio.RecordResponseAddText(builder, textOff)
		builder.Finish(protoio.RecordResponseEnd(builder))
		return builder.FinishedBytes(), nil
	}

	model := defaultTTSModel
	if req.Model() != nil {
		model = string(req.Model())
	}

	wg := &sync.WaitGroup{}
	wg.Add(1)
	var wavData []byte
	stopChan, err := io.RecordAudio(44100, func(data []byte, err error) {
		defer wg.Done()
		if err != nil {
			log.Errorf("Failed to record audio: %v", err)
			return
		}

		wavData = data
	})
	if err != nil {
		log.Errorf("Failed to record audio: %v", err)
		return nil, err
	}

	// display progress bar
	bar := progressbar.NewOptions64(-1,
		progressbar.OptionSetDescription("Recording... Enter to stop"),
		progressbar.OptionSetWriter(os.Stderr),
		progressbar.OptionSetWidth(10),
		progressbar.OptionOnCompletion(func() {
			fmt.Fprint(os.Stderr, "\n")
		}),
		progressbar.OptionSpinnerType(14),
		progressbar.OptionFullWidth(),
		progressbar.OptionSetRenderBlankState(true),
	)

	// Wait for the user to press enter
	go func() {
		_, _ = bufio.NewReader(os.Stdin).ReadBytes('\n')
		close(stopChan)
	}()

	// Wait for the recording to finish
	wg.Wait()

	bar.Describe("Recorded")
	bar.Close()

	// convert wavData to text
	text, err := speechToTextString(wavData, model)
	if err != nil {
		return nil, fmt.Errorf("error converting audio to text: %w", err)
	}

	builder := flatbuffers.NewBuilder(0)
	textOff := builder.CreateString(text)

	protoio.RecordResponseStart(builder)
	protoio.RecordResponseAddText(builder, textOff)
	builder.Finish(protoio.RecordResponseEnd(builder))

	return builder.FinishedBytes(), nil
}
