CURRENT_DIR := $(shell pwd)
OUTPUT_DIR := $(shell pwd)/bin
PROJ_NAME := $(shell basename $(CURRENT_DIR))

all: build

build:
	docker compose build

clean:
	rm -rf $(OUTPUT_DIR)

.PHONY: all build
