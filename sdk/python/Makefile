.PHONY: all build clean

CURRENT_DIR := $(shell pwd)

all: clean build

build:
	python -m build

clean:
	rm -rf $(CURRENT_DIR)/dist $(CURRENT_DIR)/spear.egg-info
