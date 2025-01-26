.PHONY: all build clean

CURRENT_DIR := $(shell pwd)
REPO_ROOT := $(shell git rev-parse --show-toplevel)

all: include/proto

include/proto:
	allfiles=`find ${REPO_ROOT}/proto -name "*.fbs"`; \
	flatc -o ${CURRENT_DIR}/include/proto -I ${REPO_ROOT}/proto --cpp --gen-all $${allfiles}

clean:
	rm -rf ${CURRENT_DIR}/include/proto;

test:
	@echo "No test for cpp sdk"

.PHONY: all build clean test
