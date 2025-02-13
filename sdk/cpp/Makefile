.PHONY: all build clean

CURRENT_DIR := $(shell pwd)
REPO_ROOT := $(shell git rev-parse --show-toplevel)

FLATC := $(shell command -v flatc 2> /dev/null)

ifndef FLATC
  $(error "flatc binary not found in PATH. Please install flatc.")
endif

all: include/proto

include/proto:
	allfiles=`find ${REPO_ROOT}/proto -name "*.fbs"`; \
	$(FLATC) -o ${CURRENT_DIR}/include/proto -I ${REPO_ROOT}/proto --cpp --gen-all $${allfiles}

clean:
	rm -rf ${CURRENT_DIR}/include/proto;

test:
	@echo "No test for cpp sdk"

.PHONY: all build clean test
