.PHONY: all build clean

CURRENT_DIR := $(shell pwd)
REPO_ROOT := $(shell git rev-parse --show-toplevel)
VERSION := $(shell git describe --tags --always --dirty | sed 's/-dirty/+dirty/')

all: clean build

build: spear/proto
	sed -i '' "s/version = .*/version = \"${VERSION}\"/g" ${REPO_ROOT}/sdk/python/pyproject.toml; \
	python3 -m pip install -r requirements.txt; \
	python3 -m build

spear/proto:
	allfiles=`find ${REPO_ROOT}/proto -name "*.fbs"`; \
	flatc -o ${CURRENT_DIR}/ -I ${REPO_ROOT}/proto --python --python-typing --gen-all $${allfiles}

clean:
	rm -rf ${CURRENT_DIR}/spear/proto && \
	rm -rf $(CURRENT_DIR)/dist $(CURRENT_DIR)/spear.egg-info && \
	find $(CURRENT_DIR) | grep -E "(__pycache__|\.pyc$$)" | xargs rm -rf

install: build
	pip uninstall spear -y; \
	pip install $(CURRENT_DIR)/dist/spear-*.whl

uninstall:
	pip uninstall spear -y

test: build
	PYTHONPATH=$(CURRENT_DIR) pytest --log-cli-level=DEBUG -s tests/
