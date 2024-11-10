#!/bin/sh

# a json rpc example

for i in `seq 1 2`;
do
    echo "{\"jsonrpc\":\"2.0\",\"method\":\"eth_blockNumber\",\"params\":[],\"id\":83}"
    sleep 10
done
