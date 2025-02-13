package main

import (
	"encoding/binary"
	"fmt"
	"io"
	"net"
	"os"
	"strconv"
	"time"

	flatbuffers "github.com/google/flatbuffers/go"
	spearnet "github.com/lfedgeai/spear/pkg/net"
	"github.com/lfedgeai/spear/pkg/spear/proto/custom"

	log "github.com/sirupsen/logrus"
)

var hdl *spearnet.GuestRPCManager
var hostaddr string
var secret string

var input io.Reader
var output io.Writer

// parse arguments
func init() {
	// get hostaddr and secret from environment variables
	hostaddr = os.Getenv("SERVICE_ADDR")
	secret = os.Getenv("SECRET")

	log.Debugf("Connecting to host at %s", hostaddr)
	// create tcp connection to host
	conn, err := net.Dial("tcp", hostaddr)
	if err != nil {
		log.Fatalf("failed to connect to host: %v", err)
	}

	// sending the secret
	// convert secret string to int64
	secretInt, err := strconv.ParseInt(secret, 10, 64)
	if err != nil {
		log.Fatalf("failed to convert secret to int64: %v", err)
	}
	// convert int64 to little endian byte array
	secretBytes := make([]byte, 8)
	binary.LittleEndian.PutUint64(secretBytes, uint64(secretInt))
	// write secret to connection
	_, err = conn.Write(secretBytes)
	if err != nil {
		log.Fatalf("failed to write secret to connection: %v", err)
	}

	// create input and output files from connection
	input = conn
	output = conn
}

func main() {
	hdl = spearnet.NewGuestRPCManager()
	hdl.SetInput(input)
	hdl.SetOutput(output)

	done := make(chan bool)

	hdl.RegisterIncomingCustomRequestHandler("handle",
		func(args *custom.CustomRequest) (*custom.CustomResponse, error) {
			defer func() {
				done <- true
			}()
			log.Debugf("Incoming request: %v", args)
			if args.RequestInfoType() != custom.RequestInfoNormalRequestInfo {
				log.Errorf("we do not support types other than NormalReqeustInfo.")
				return nil, fmt.Errorf("we do not support types other than NormalReqeustInfo.")
			}

			tbl := flatbuffers.Table{}
			if !args.RequestInfo(&tbl) {
				log.Errorf("failed to get table from request info.")
				return nil, fmt.Errorf("failed to get table from request info.")
			}
			req := &custom.NormalRequestInfo{}
			req.Init(tbl.Bytes, tbl.Pos)

			str := string(req.ParamsStr())
			// make sure args is a string
			resp, err := generateImage(str)
			if err != nil {
				log.Errorf("failed to generate image: %v", err)
				return nil, err
			}

			log.Debugf("Generated image: %v", resp)

			builder := flatbuffers.NewBuilder(0)
			respOff := builder.CreateByteVector(resp)

			custom.CustomResponseStart(builder)
			custom.CustomResponseAddData(builder, respOff)
			builder.Finish(custom.CustomResponseEnd(builder))

			respBytes := builder.FinishedBytes()

			customResp := custom.GetRootAsCustomResponse(respBytes, 0)
			return customResp, nil
		})
	go hdl.Run()

	<-done
	log.Debug("Exiting")
	time.Sleep(5 * time.Second)
}

func generateImage(str string) ([]byte, error) {
	log.Infof("this test is temporarily disabled")

	// res, err := rpc.TextToImage(hdl, "dall-e-3", str, "b64_json")
	// if err != nil {
	// 	return nil, fmt.Errorf("failed to generate image: %v", err)
	// }
	// return res, nil
	return nil, nil
}
