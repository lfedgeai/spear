package utils

import "encoding/json"

func InterfaceToType[T any](d *T, s interface{}) error {
	// first marshal to json
	jsonData, err := json.Marshal(s)
	if err != nil {
		return err
	}
	// then unmarshal to T
	err = json.Unmarshal(jsonData, d)
	if err != nil {
		return err
	}
	return nil
}
