#!/bin/bash

set -x
set -e

SRC_DIR="$( cd "$( dirname "${BASH_SOURCE[0]}" )" &> /dev/null && pwd )"

screen -dmS testsession

screen -S testsession -X screen bash -c \
    "cd ${SRC_DIR}; source ../lib.source; start_server -L ${SRC_DIR}; wait_keypress"

sleep 10
echo "inserting qwen"
curl --location 'http://localhost:8081/model/0' \
--header 'Content-Type: application/json' \
--data '{
    "name": "qwen2.5-7B",
    "model": "Qwen/Qwen2.5-7B-Instruct",
    "base": "http://localhost:9000/v1/",
    "apikey": "",
    "apikey_in_env": "",
    "url": "/chat/completions"
}'

sleep 2
curl --insecure --location 'https://localhost:8080' \
--header 'Spear-Func-Type: 2' \
--header 'Spear-Func-Name: test.py'
