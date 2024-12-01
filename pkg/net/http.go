package net

import (
	"bytes"
	"fmt"
	"io"
	"net/http"
)

type ContentType int

const (
	ContentTypeJSON ContentType = iota
	ContentTypeText
)

func SendRequest(url string, data *bytes.Buffer, contentType ContentType, apiKey string) ([]byte, error) {
	// create a https request to url and use data as the request body
	req, err := http.NewRequest("POST", url, data)
	if err != nil {
		return nil, fmt.Errorf("error creating request: %v", err)
	}

	// set the headers
	switch contentType {
	case ContentTypeJSON:
		req.Header.Set("Content-Type", "application/json")
	case ContentTypeText:
		req.Header.Set("Content-Type", "text/plain")
	}
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
