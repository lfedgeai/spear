
VERSION := $(shell git describe --tags --match "*" --always --dirty)
REPO_ROOT := $(shell pwd)
OUTPUT_DIR := $(REPO_ROOT)/bin


all: clean spearlet workload sdk


SUBDIRS := $(shell find $(REPO_ROOT) -mindepth 1 -maxdepth 3 -type d -exec test -e {}/Makefile \; -exec echo {} \;)
WORKLOAD_SUBDIRS := $(shell find $(REPO_ROOT)/workload -mindepth 1 -maxdepth 3 -type d -exec test -e {}/Makefile \; -exec echo {} \;)

clean:
	@set -ex; \
	docker system prune -f && \
	rm -rf $(OUTPUT_DIR) && \
	rm -rf $(REPO_ROOT)/pkg/spear && \
	for dir in $(SUBDIRS); do \
		make -C $$dir clean; \
	done

build: spearlet
	@set -e; \
	for dir in $(SUBDIRS); do \
		make -C $$dir build; \
	done

spearlet: pkg/spear
	go build -o $(OUTPUT_DIR)/spearlet \
	-ldflags "-X 'github.com/lfedgeai/spear/pkg/common.Version=$(VERSION)'" \
	$(REPO_ROOT)/cmd/spearlet/main.go

test: workload
	@set -e; \
	cd $(REPO_ROOT); \
	go test -v ./test/... && \
	for dir in $(SUBDIRS); do \
		make -C $$dir test; \
	done; \

workload: build
	@set -e; \
	for dir in $(WORKLOAD_SUBDIRS); do \
		make -C $$dir; \
	done

format_python:
	isort -rc $(REPO_ROOT)/

format_golang:
	gofmt -w .

format: format_python format_golang

pkg/spear:
	allfiles=`find ${REPO_ROOT}/proto -name "*.fbs"`; \
	flatc -o $(REPO_ROOT)/pkg/ -I ${REPO_ROOT}/proto --go-module-name "github.com/lfedgeai/spear/pkg" --go --gen-all $${allfiles}

.PHONY: all spearlet test workload clean format_python format
