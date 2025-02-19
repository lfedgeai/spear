CURRENT_DIR := $(shell pwd)
OUTPUT_DIR := $(shell pwd)/bin
PROJ_NAME := $(shell basename $(CURRENT_DIR))
SDK_OUTPUT_DIR := $(shell pwd)/../../../../sdk/python/dist/
WHL_FILENAME := $(shell ls $(SDK_OUTPUT_DIR)/../dist/*.whl | xargs basename | tail -n 1)

all: build

build: demo
	docker compose build --no-cache --build-arg WHL_FILENAME=${WHL_FILENAME}

demo:
	go build -o $(OUTPUT_DIR)/demo \
	$(CURRENT_DIR)/src/demo.go

clean:
	rm -rf $(OUTPUT_DIR)

.PHONY: all build clean demo
