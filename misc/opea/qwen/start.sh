#!/bin/bash

set -x
set -e

SRC_DIR="$( cd "$( dirname "${BASH_SOURCE[0]}" )" &> /dev/null && pwd )"
WAIT_TIME_SEC="5"
SESSION_NAME="testsession"
TIMESTAMP=$(date +"%Y%m%d_%H%M%S")

# killing existing session
screen -S "${SESSION_NAME}" -X quit 2>/dev/null || true

# creating new session
screen -dmS "${SESSION_NAME}"

echo "starting spear server..."
screen -S "${SESSION_NAME}" -X screen -L bash -c \
    "cd ${SRC_DIR}; source ../lib.source; start_server -L ${SRC_DIR}; wait_keypress"

echo "waiting ${WAIT_TIME_SEC} seconds for the server to get ready..."
sleep ${WAIT_TIME_SEC}

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

echo "waiting ${WAIT_TIME_SEC} seconds to trigger test.py execution on spear..."
sleep ${WAIT_TIME_SEC}
curl --insecure --location 'https://localhost:8080' \
--header 'Spear-Func-Type: 2' \
--header 'Spear-Func-Name: test.py'

echo "Remember to clean up the session using command \"screen -S \'${SESSION_NAME}\' -X quit\""
