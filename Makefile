
PROJECT_ROOT := $(shell pwd)
OUTPUT_DIR := $(PROJECT_ROOT)/bin


all: clean worker workload sdk

clean:
	rm -rf $(OUTPUT_DIR) && \
	find $(PROJECT_ROOT)/workload -mindepth 1 -maxdepth 3 -type d -exec test -e {}/Makefile \; -exec make -C {} clean \;
	find $(PROJECT_ROOT)/sdk -mindepth 1 -maxdepth 2 -type d -exec test -e {}/Makefile \; -exec make -C {} clean \;

worker:
	go build -o $(OUTPUT_DIR)/worker \
	$(PROJECT_ROOT)/cmd/worker/main.go

test: workload
	go test -v $(PROJECT_ROOT)/test/...

workload: sdk
	find $(PROJECT_ROOT)/workload -mindepth 1 -maxdepth 3 -type d -exec test -e {}/Makefile \; -exec echo "make -C {}" \; -exec make -C {} \;

sdk:
	find $(PROJECT_ROOT)/sdk -mindepth 1 -maxdepth 2 -type d -exec test -e {}/Makefile \; -exec make -C {} \;

format_python:
	isort -rc $(PROJECT_ROOT)/

format: format_python

.PHONY: all worker test workload clean sdk format_python format
