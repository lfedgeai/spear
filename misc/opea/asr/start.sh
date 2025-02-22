#!/bin/bash

set -x
set -e

SRC_DIR="$( cd "$( dirname "${BASH_SOURCE[0]}" )" &> /dev/null && pwd )"

screen -dmS testsession

screen -S testsession -X screen bash -c \
    "cd ${SRC_DIR}; source ../lib.source; start_server -L ${SRC_DIR}; wait_keypress"

sleep 10
echo "inserting ASR"
curl --location 'http://localhost:8081/model/4' \
--header 'Content-Type: application/json' \
--data '{
    "name": "whisper-small",
    "model": "Whisper-small",
    "base": "http://localhost:9099/v1",
    "apikey": "",
    "apikey_in_env": "",
    "url": "/audio/transcriptions"
}'

sleep 2
curl --insecure --location 'https://localhost:8080' \
--header 'Spear-Func-Type: 2' \
--header 'Spear-Func-Name: test.py'
