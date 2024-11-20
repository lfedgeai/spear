package hostcalls

import (
	"bytes"
	"fmt"
	"io"
	"net/http"
	"os"
)

func sendBufferData(data *bytes.Buffer, url string) ([]byte, error) {
	// create a https request to url and use data as the request body
	req, err := http.NewRequest("POST", url, data)
	if err != nil {
		return nil, fmt.Errorf("error creating request: %v", err)
	}

	// get api key from environment variable
	apiKey := os.Getenv("OPENAI_API_KEY")
	// set the headers
	req.Header.Set("Content-Type", "application/json")
	req.Header.Set("Authorization", fmt.Sprintf("Bearer %s", apiKey))
	// send the request
	client := &http.Client{}
	resp, err := client.Do(req)
	if err != nil {
		return nil, fmt.Errorf("error sending request: %v", err)
	}
	defer resp.Body.Close()

	body, err := io.ReadAll(resp.Body)
	if err != nil {
		return nil, fmt.Errorf("error reading response: %v", err)
	}

	return body, nil
}
