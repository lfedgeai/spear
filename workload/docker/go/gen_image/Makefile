.PHONY: all build clean

CURRENT_DIR := $(shell pwd)
OUTPUT_DIR := $(shell pwd)/bin
PROJ_NAME := $(shell basename $(CURRENT_DIR))

all: build
	docker compose build --no-cache

start:
	CGO_ENABLED=0 GOOS=linux go build -o $(OUTPUT_DIR)/start \
	$(CURRENT_DIR)/src/start.go

build: start

clean:
	rm -rf $(OUTPUT_DIR)

.PHONY: all build clean
