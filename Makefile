
PROJECT_ROOT := $(shell pwd)
OUTPUT_DIR := $(PROJECT_ROOT)/bin


all: clean worker workload demo

clean:
	rm -rf $(OUTPUT_DIR) && \
	find $(PROJECT_ROOT)/workload -mindepth 1 -maxdepth 2 -type d -exec test -e {}/Makefile \; -exec make -C {} clean \;

worker:
	go build -o $(OUTPUT_DIR)/worker \
	$(PROJECT_ROOT)/cmd/worker/main.go

test: workload
	go test -v $(PROJECT_ROOT)/test/simple_req_test.go

workload:
	find $(PROJECT_ROOT)/workload -mindepth 1 -maxdepth 2 -type d -exec test -e {}/Makefile \; -exec make -C {} \;

.PHONY: all worker test workload clean
