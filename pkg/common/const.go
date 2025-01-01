package common

import "runtime"

const MaxDataResponseSize = 4096 * 1024

var SpearPlatformAddress string

func init() {
	SpearPlatformAddress = map[string]string{
		"darwin":  "host.docker.internal",
		"linux":   "172.17.0.1",
		"windows": "host.docker.internal",
	}[runtime.GOOS]
}
