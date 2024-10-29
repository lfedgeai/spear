package hostcalls

import (
	"context"
	"encoding/json"
	"fmt"

	"github.com/lfedgeai/spear/pkg/rpc/payload"
	hostcalls "github.com/lfedgeai/spear/worker/hostcalls/common"
	"github.com/lfedgeai/spear/worker/task"
	"github.com/qdrant/go-client/qdrant"
	log "github.com/sirupsen/logrus"
)

var (
	globalVectorStoreRegistries = make(map[task.TaskID]*VectorStoreRegistry)
)

type VectorStore struct {
	Name string
}

type VectorStoreRegistry struct {
	Stores []*VectorStore
	Client *qdrant.Client
}

func NewVectorStoreRegistry() (*VectorStoreRegistry, error) {
	qdrantClient, err := qdrant.NewClient(&qdrant.Config{
		Host: "localhost",
		Port: 6334,
	})
	if err != nil {
		log.Errorf("Error creating qdrant client: %v", err)
		return nil, err
	}
	// list all collections
	collections, err := qdrantClient.ListCollections(context.Background())
	if err != nil {
		log.Errorf("Error listing collections: %v", err)
		return nil, err
	}
	log.Infof("Collections: %v", collections)
	return &VectorStoreRegistry{
		Stores: make([]*VectorStore, 0),
		Client: qdrantClient,
	}, nil
}

func (r *VectorStoreRegistry) Create(storeName string) (int, error) {
	log.Infof("Creating vector store with name %s", storeName)
	// duplicated store is not allowed
	for i, store := range r.Stores {
		if store.Name == storeName {
			return i, fmt.Errorf("store with name %s already exists", storeName)
		}
	}

	// create the vector store in qdrant
	err := r.Client.CreateCollection(context.Background(), &qdrant.CreateCollection{
		CollectionName: storeName,
		VectorsConfig: qdrant.NewVectorsConfig(&qdrant.VectorParams{
			Size:     4,
			Distance: qdrant.Distance_Cosine,
		}),
	})
	if err != nil {
		return -1, fmt.Errorf("error creating collection: %v", err)
	}

	// create a new vector store with the given name
	r.Stores = append(r.Stores, &VectorStore{
		Name: storeName,
	})

	return len(r.Stores) - 1, nil
}

func (r *VectorStoreRegistry) Delete(vid int) error {
	log.Infof("Deleting vector store with id %d", vid)
	// delete the vector store in qdrant
	err := r.Client.DeleteCollection(context.Background(), r.Stores[vid].Name)
	if err != nil {
		return fmt.Errorf("error deleting collection: %v", err)
	}

	// remove the vid-th vector store
	r.Stores = append(r.Stores[:vid], r.Stores[vid+1:]...)

	return nil
}

func VectorStoreCreate(caller *hostcalls.Caller, args interface{}) (interface{}, error) {
	task := *(caller.Task)
	log.Infof("Executing hostcall \"%s\" with args %v", payload.HostCallVectorStoreCreate, args)
	// verify the type of args is string
	// use json marshal and unmarshal to verify the type
	jsonBytes, err := json.Marshal(args)
	if err != nil {
		return nil, fmt.Errorf("error marshalling args: %v", err)
	}
	req := payload.VectorStoreCreateRequest{}
	err = req.Unmarshal(jsonBytes)
	if err != nil {
		return nil, fmt.Errorf("error unmarshalling args: %v", err)
	}

	log.Infof("VectorStoreCreate Request: %v", req)
	// create a new vector store
	if _, ok := globalVectorStoreRegistries[task.ID()]; !ok {
		val, err := NewVectorStoreRegistry()
		if err != nil {
			return nil, fmt.Errorf("error creating vector store registry: %v", err)
		}
		globalVectorStoreRegistries[task.ID()] = val
	}

	vid, err := globalVectorStoreRegistries[task.ID()].Create(req.Name)
	if err != nil {
		return nil, fmt.Errorf("error creating vector store: %v", err)
	}

	// return the response
	return &payload.VectorStoreCreateResponse{
		VID: vid,
	}, nil
}

func VectorStoreDelete(caller *hostcalls.Caller, args interface{}) (interface{}, error) {
	task := *(caller.Task)
	log.Infof("Executing hostcall \"%s\" with args %v", payload.HostCallVectorStoreDelete, args)
	// verify the type of args is int
	// use json marshal and unmarshal to verify the type
	jsonBytes, err := json.Marshal(args)
	if err != nil {
		return nil, fmt.Errorf("error marshalling args: %v", err)
	}
	req := payload.VectorStoreDeleteRequest{}
	err = req.Unmarshal(jsonBytes)
	if err != nil {
		return nil, fmt.Errorf("error unmarshalling args: %v", err)
	}

	log.Infof("VectorStoreDelete Request: %v", req)
	// delete the vector store
	if _, ok := globalVectorStoreRegistries[task.ID()]; !ok {
		return nil, fmt.Errorf("vector store registry not found")
	}

	err = globalVectorStoreRegistries[task.ID()].Delete(req.VID)
	if err != nil {
		return nil, fmt.Errorf("error deleting vector store: %v", err)
	}

	// return the response
	return &payload.VectorStoreDeleteResponse{
		VID: req.VID,
	}, nil
}

func VectorStoreInsert(caller *hostcalls.Caller, args interface{}) (interface{}, error) {
	task := *(caller.Task)
	log.Infof("Executing hostcall \"%s\" with args %v", payload.HostCallVectorStoreInsert, args)
	// verify the type of args is VectorStoreInsertRequest
	// use json marshal and unmarshal to verify the type
	jsonBytes, err := json.Marshal(args)
	if err != nil {
		return nil, fmt.Errorf("error marshalling args: %v", err)
	}
	req := payload.VectorStoreInsertRequest{}
	err = req.Unmarshal(jsonBytes)
	if err != nil {
		return nil, fmt.Errorf("error unmarshalling args: %v", err)
	}

	log.Infof("VectorStoreInsert Request: %s", string(jsonBytes))
	// insert the vector into the vector store
	if _, ok := globalVectorStoreRegistries[task.ID()]; !ok {
		return nil, fmt.Errorf("vector store registry not found")
	}

	// return the response
	return payload.VectorStoreInsertResponse{
		VID: req.VID,
	}, nil
}
