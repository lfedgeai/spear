.PHONY: all build clean

CURRENT_DIR := $(shell pwd)
REPO_ROOT := $(shell git rev-parse --show-toplevel)

all: clean build

build: spear/proto
	python3 -m pip install -r requirements.txt; \
	python3 -m build

spear/proto:
	allfiles=`find ${REPO_ROOT}/proto -name "*.fbs"`; \
	flatc -o ${CURRENT_DIR}/ -I ${REPO_ROOT}/proto --python --python-typing $${allfiles}

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
