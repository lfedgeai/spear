package hostcalls

import (
	"context"
	"fmt"

	hostcalls "github.com/lfedgeai/spear/spearlet/hostcalls/common"
	"github.com/lfedgeai/spear/spearlet/task"
	"github.com/qdrant/go-client/qdrant"
	log "github.com/sirupsen/logrus"
)

var (
	globalVectorStoreRegistries = make(map[task.TaskID]*VectorStoreRegistry)
)

type VectorStore struct {
	Name   string
	NextID uint64
}

type VectorStoreRegistry struct {
	Stores []*VectorStore
	Client *qdrant.Client
}

type VectorStoreSearchResult struct {
	Vector []float32
	Data   []byte
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

func (r *VectorStoreRegistry) Create(storeName string, dimensions uint64) (int, error) {
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
			Size:     dimensions,
			Distance: qdrant.Distance_Cosine,
		}),
	})
	if err != nil {
		return -1, fmt.Errorf("error creating collection: %v", err)
	}

	// create a new vector store with the given name
	r.Stores = append(r.Stores, &VectorStore{
		Name:   storeName,
		NextID: 1,
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

func (r *VectorStoreRegistry) Insert(vid int, vector []float32, payload []byte) error {
	log.Infof("Inserting vector into vector store with id %d", vid)
	// insert the vector into qdrant
	opInfo, err := r.Client.Upsert(context.Background(), &qdrant.UpsertPoints{
		CollectionName: r.Stores[vid].Name,
		Points: []*qdrant.PointStruct{
			{
				Id: qdrant.NewIDNum(r.Stores[vid].NextID),
				Payload: qdrant.NewValueMap(map[string]interface{}{
					"payload": payload,
				}),
				Vectors: qdrant.NewVectors(vector...),
			},
		},
	})
	if err != nil {
		return fmt.Errorf("error upserting points: %v", err)
	}
	r.Stores[vid].NextID = r.Stores[vid].NextID + 1
	log.Infof("Upsert operation info: %v", opInfo)
	return nil
}

func (r *VectorStoreRegistry) Search(vid int, vector []float32, limit uint64) ([]*VectorStoreSearchResult, error) {
	log.Infof("Searching vector in vector store with vid %d and vector %v", vid, vector)
	// search the vector in qdrant
	result, err := r.Client.Query(context.Background(), &qdrant.QueryPoints{
		CollectionName: r.Stores[vid].Name,
		Query:          qdrant.NewQuery(vector...),
		Limit:          &limit,
	})
	if err != nil {
		return nil, fmt.Errorf("error querying points: %v", err)
	}
	ret := make([]*VectorStoreSearchResult, len(result))
	for i, res := range result {
		if res.Vectors == nil {
			log.Infof(fmt.Sprintf("Vector is nil: %v", res))
			ret[i] = &VectorStoreSearchResult{
				Vector: nil,
				Data:   []byte(res.Payload["payload"].String()),
			}
		} else {
			ret[i] = &VectorStoreSearchResult{
				Vector: res.Vectors.GetVector().Data,
				Data:   []byte(res.Payload["payload"].String()),
			}
		}
	}
	log.Infof("Search result: %+v", ret)
	return ret, nil
}

func VectorStoreCreate(inv *hostcalls.InvocationInfo, args []byte) ([]byte, error) {
	// task := *(inv.Task)
	// log.Debugf("Executing hostcall \"%s\" with args %v", payload.HostCallVectorStoreCreate, args)
	// // verify the type of args is string
	// // use json marshal and unmarshal to verify the type
	// jsonBytes, err := json.Marshal(args)
	// if err != nil {
	// 	return nil, fmt.Errorf("error marshalling args: %v", err)
	// }
	// req := payload.VectorStoreCreateRequest{}
	// err = req.Unmarshal(jsonBytes)
	// if err != nil {
	// 	return nil, fmt.Errorf("error unmarshalling args: %v", err)
	// }

	// log.Infof("VectorStoreCreate Request: %v", req)
	// // create a new vector store
	// if _, ok := globalVectorStoreRegistries[task.ID()]; !ok {
	// 	val, err := NewVectorStoreRegistry()
	// 	if err != nil {
	// 		return nil, fmt.Errorf("error creating vector store registry: %v", err)
	// 	}
	// 	globalVectorStoreRegistries[task.ID()] = val
	// }

	// vid, err := globalVectorStoreRegistries[task.ID()].Create(req.Name, req.Dimentions)
	// if err != nil {
	// 	return nil, fmt.Errorf("error creating vector store: %v", err)
	// }

	// // return the response
	// return &payload.VectorStoreCreateResponse{
	// 	VID: vid,
	// }, nil

	return nil, fmt.Errorf("not implemented")
}

func VectorStoreDelete(inv *hostcalls.InvocationInfo, args []byte) ([]byte, error) {
	// task := *(inv.Task)
	// log.Debugf("Executing hostcall \"%s\" with args %v", payload.HostCallVectorStoreDelete, args)
	// // verify the type of args is int
	// // use json marshal and unmarshal to verify the type
	// jsonBytes, err := json.Marshal(args)
	// if err != nil {
	// 	return nil, fmt.Errorf("error marshalling args: %v", err)
	// }
	// req := payload.VectorStoreDeleteRequest{}
	// err = req.Unmarshal(jsonBytes)
	// if err != nil {
	// 	return nil, fmt.Errorf("error unmarshalling args: %v", err)
	// }

	// log.Infof("VectorStoreDelete Request: %v", req)
	// // delete the vector store
	// if _, ok := globalVectorStoreRegistries[task.ID()]; !ok {
	// 	return nil, fmt.Errorf("vector store registry not found")
	// }

	// err = globalVectorStoreRegistries[task.ID()].Delete(req.VID)
	// if err != nil {
	// 	return nil, fmt.Errorf("error deleting vector store: %v", err)
	// }

	// // return the response
	// return &payload.VectorStoreDeleteResponse{
	// 	VID: req.VID,
	// }, nil

	return nil, fmt.Errorf("not implemented")
}

func VectorStoreInsert(inv *hostcalls.InvocationInfo, args []byte) ([]byte, error) {
	// task := *(inv.Task)
	// log.Debugf("Executing hostcall \"%s\" with args %v", payload.HostCallVectorStoreInsert, args)
	// // verify the type of args is VectorStoreInsertRequest
	// // use json marshal and unmarshal to verify the type
	// jsonBytes, err := json.Marshal(args)
	// if err != nil {
	// 	return nil, fmt.Errorf("error marshalling args: %v", err)
	// }
	// req := payload.VectorStoreInsertRequest{}
	// err = req.Unmarshal(jsonBytes)
	// if err != nil {
	// 	return nil, fmt.Errorf("error unmarshalling args: %v", err)
	// }

	// log.Infof("VectorStoreInsert Request: %s", string(jsonBytes))
	// // insert the vector into the vector store
	// v, ok := globalVectorStoreRegistries[task.ID()]
	// if !ok {
	// 	return nil, fmt.Errorf("vector store registry not found")
	// }

	// err = v.Insert(req.VID, req.Vector, req.Data)
	// if err != nil {
	// 	return nil, fmt.Errorf("error inserting vector: %v", err)
	// }

	// // return the response
	// return payload.VectorStoreInsertResponse{
	// 	VID: req.VID,
	// }, nil

	return nil, fmt.Errorf("not implemented")
}

func VectorStoreSearch(inv *hostcalls.InvocationInfo, args []byte) ([]byte, error) {
	// task := *(inv.Task)
	// log.Debugf("Executing hostcall \"%s\" with args %v", payload.HostCallVectorStoreSearch, args)
	// // verify the type of args is VectorStoreSearchRequest
	// // use json marshal and unmarshal to verify the type
	// jsonBytes, err := json.Marshal(args)
	// if err != nil {
	// 	return nil, fmt.Errorf("error marshalling args: %v", err)
	// }
	// req := payload.VectorStoreSearchRequest{}
	// err = req.Unmarshal(jsonBytes)
	// if err != nil {
	// 	return nil, fmt.Errorf("error unmarshalling args: %v", err)
	// }

	// log.Infof("VectorStoreSearch Request: %s", string(jsonBytes))
	// // search the vector in the vector store
	// v, ok := globalVectorStoreRegistries[task.ID()]
	// if !ok {
	// 	return nil, fmt.Errorf("vector store registry not found")
	// }

	// result, err := v.Search(req.VID, req.Vector, req.Limit)
	// if err != nil {
	// 	return nil, fmt.Errorf("error searching vector: %v", err)
	// }

	// // return the response
	// res := payload.VectorStoreSearchResponse{
	// 	VID:     req.VID,
	// 	Entries: make([]payload.VectorStoreSearchResponseEntry, len(result)),
	// }
	// for i, r := range result {
	// 	res.Entries[i] = payload.VectorStoreSearchResponseEntry{
	// 		Vector: r.Vector,
	// 		Data:   r.Data,
	// 	}
	// }
	// return res, nil

	return nil, fmt.Errorf("not implemented")
}
