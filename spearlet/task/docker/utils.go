package docker

import (
	"context"
	"fmt"
	"io"
	"time"

	"bytes"

	"github.com/docker/docker/api/types/container"
	"github.com/docker/docker/api/types/image"
	"github.com/docker/docker/client"
	"github.com/docker/go-connections/nat"
)

func StopContainer(cid string) error {
	// stop the vector store container
	cli, err := client.NewClientWithOpts(client.FromEnv)
	if err != nil {
		return err
	}

	err = cli.ContainerStop(context.TODO(), cid, container.StopOptions{})
	if err != nil {
		return err
	}

	err = cli.ContainerRemove(context.TODO(), cid, container.RemoveOptions{})
	if err != nil {
		return err
	}
	return nil
}

func StartVectorStoreContainer(cleanup bool, pull bool) (*container.CreateResponse, error) {
	// start the vector store container
	// docker run -p 6333:6333 -p 6334:6334 \
	// -v $(pwd)/qdrant_storage:/qdrant/storage:z \
	// qdrant/qdrant

	// start docker client
	cli, err := client.NewClientWithOpts(client.FromEnv)
	if err != nil {
		return nil, err
	}

	// pull the image
	if pull {
		r, err := cli.ImagePull(context.TODO(), "docker.io/qdrant/qdrant", image.PullOptions{})
		if err != nil {
			return nil, err
		}

		// read the response
		buf := new(bytes.Buffer)
		_, err = io.Copy(buf, r)
		if err != nil {
			return nil, err
		}
	}

	// delete all existing qdrant containers
	if cleanup {
		containers, err := cli.ContainerList(context.TODO(), container.ListOptions{})
		if err != nil {
			return nil, err
		}
		for _, c := range containers {
			if c.Image == "qdrant/qdrant" {
				err = cli.ContainerStop(context.TODO(), c.ID, container.StopOptions{})
				if err != nil {
					return nil, err
				}
				err = cli.ContainerRemove(context.TODO(), c.ID, container.RemoveOptions{})
				if err != nil {
					return nil, err
				}
			}
		}
	}

	// generate a random name for the container
	name := fmt.Sprintf("qdrant-%d", time.Now().Unix())

	// create the container
	c, err := cli.ContainerCreate(context.TODO(), &container.Config{
		Image: "qdrant/qdrant",
	}, &container.HostConfig{
		PortBindings: nat.PortMap{
			"6333/tcp": []nat.PortBinding{
				{
					HostIP:   "localhost",
					HostPort: "6333",
				},
			},
			"6334/tcp": []nat.PortBinding{
				{
					HostIP:   "localhost",
					HostPort: "6334",
				},
			},
		},
	}, nil, nil, name)
	if err != nil {
		return nil, err
	}

	// start the container
	err = cli.ContainerStart(context.TODO(), c.ID, container.StartOptions{})
	if err != nil {
		return nil, err
	}

	// wait for the container to start
	time.Sleep(5 * time.Second)

	// check the container status
	info, err := cli.ContainerInspect(context.TODO(), c.ID)
	if err != nil {
		return nil, err
	}

	if !info.State.Running {
		return nil, fmt.Errorf("container not running")
	}

	return &c, nil
}
