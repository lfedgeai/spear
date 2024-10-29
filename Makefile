
PROJECT_ROOT := $(shell pwd)
OUTPUT_DIR := $(PROJECT_ROOT)/bin


all: clean worker workload

clean:
	rm -rf $(OUTPUT_DIR)

worker:
	go build -o $(OUTPUT_DIR)/worker \
	$(PROJECT_ROOT)/cmd/worker/main.go

test:
	go test $(PROJECT_ROOT)/test/simple_req_test.go

workload:
	find $(PROJECT_ROOT)/workload -mindepth 1 -maxdepth 2 -type d -exec test -e {}/Makefile \; -exec make -C {} \;

.PHONY: all worker test workload clean
