
PROJECT_ROOT := $(shell pwd)
OUTPUT_DIR := $(PROJECT_ROOT)/bin

all: clean dummy worker

clean:
	rm -rf $(OUTPUT_DIR)

worker:
	go build -o $(OUTPUT_DIR)/worker $(PROJECT_ROOT)/cmd/worker/worker.go

dummy:
	go build -o $(OUTPUT_DIR)/dummy_task $(PROJECT_ROOT)/cmd/dummy_task/dummy_task.go

test:
	go test $(PROJECT_ROOT)/test/simple_req_test.go

.PHONY: all worker dummy test
