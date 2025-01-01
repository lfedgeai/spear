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
	ContentTypeMultipart
)

func SendRequest(url string, data *bytes.Buffer, contentType interface{}, apiKey string) ([]byte, error) {
	// create a https request to url and use data as the request body
	req, err := http.NewRequest("POST", url, data)
	if err != nil {
		return nil, fmt.Errorf("error creating request: %v", err)
	}

	switch typ := contentType.(type) {
	case ContentType:
		switch typ {
		case ContentTypeJSON:
			req.Header.Set("Content-Type", "application/json")
		case ContentTypeText:
			req.Header.Set("Content-Type", "text/plain")
		case ContentTypeMultipart:
			req.Header.Set("Content-Type", "multipart/form-data")
		}
	case string:
		req.Header.Set("Content-Type", typ)
	default:
		return nil, fmt.Errorf("invalid content type: %v", contentType)
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
