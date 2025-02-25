package docker

import (
	"os"
	"time"

	"github.com/lfedgeai/spear/pkg/common"
	"github.com/lfedgeai/spear/spearlet"
	"github.com/lfedgeai/spear/spearlet/task/docker"
	log "github.com/sirupsen/logrus"

	"github.com/docker/docker/api/types/container"
)

type TestSetup struct {
	w        *spearlet.Spearlet
	vecStore *container.CreateResponse
}

func (t *TestSetup) stopVectorStoreContainer() {
	err := docker.StopContainer(t.vecStore.ID)
	if err != nil {
		log.Warnf("%v", err)
	}
}

func (t *TestSetup) startVectorStoreContainer() {
	c, err := docker.StartVectorStoreContainer(true, false)
	if err != nil {
		panic(err)
	}

	t.vecStore = c
}

func NewTestSetup() *TestSetup {
	// check OPENAI_API_KEY environment variable
	if os.Getenv("OPENAI_API_KEY") == "" {
		panic("OPENAI_API_KEY environment variable not set")
	}

	t := &TestSetup{}

	t.startVectorStoreContainer()

	// setup the test environment
	cfg, err := spearlet.NewServeSpearletConfig("localhost", "8080", []string{}, true,
		common.SpearPlatformAddress, "", "", true)
	if err != nil {
		panic(err)
	}
	t.w = spearlet.NewSpearlet(cfg)
	t.w.Initialize()
	go t.w.StartServer()
	time.Sleep(5 * time.Second)
	return t
}

func (t *TestSetup) TearDown() {
	// teardown the test environment
	t.w.Stop()

	t.stopVectorStoreContainer()
}
