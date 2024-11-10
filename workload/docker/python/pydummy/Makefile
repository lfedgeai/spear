CURRENT_DIR := $(shell pwd)
OUTPUT_DIR := $(shell pwd)/bin
PROJ_NAME := $(shell basename $(CURRENT_DIR))

all: build
	docker compose build

build: demo

demo:
	go build -o $(OUTPUT_DIR)/demo \
	$(CURRENT_DIR)/src/demo.go

clean:
	rm -rf $(OUTPUT_DIR)

.PHONY: all build clean demo
