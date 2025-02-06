.PHONY: all build clean

CURRENT_DIR := $(shell pwd)
REPO_ROOT := $(shell git rev-parse --show-toplevel)
VERSION := $(shell git describe --tags --match "*" --always --dirty | sed -E 's/^v//' | sed -E 's|^([0-9.]+)-([0-9]+)-g([0-9a-f]+)(-dirty)?$$|\1+\2.g\3\4|' | sed -E 's/-dirty/.dirty/')

UNAME_S := $(shell uname -s)
ifeq ($(UNAME_S),Darwin)
	SED_INPLACE = sed -i ''
else
	SED_INPLACE = sed -i
endif

all: clean build

build: spear/proto
	$(SED_INPLACE) "s/version = .*/version = \"${VERSION}\"/g" ${REPO_ROOT}/sdk/python/pyproject.toml; \
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
