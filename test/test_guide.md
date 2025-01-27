# SPEAR test

This test document will perform a simple test to verify if the overall architecture of SPEAR is functioning correctly. The testing will be conducted on a system running Ubuntu 20.04. The test process includes two parts: installation and testing.

## Installation 

### Dependencies
  SPEAR relies on some other third-party software dependency packages. To install this packages on Linux, use the following command:
  
  ```bash
  python -m pip install --upgrade pip
  pip install build
  apt install portaudio19-dev libx11-dev libxtst-dev
  curl -fsSL https://get.docker.com -o get-docker.sh
  sh get-docker.sh
  ```

### Build Instructions

To build SPEAR and its related components, run the following command:

```bash
make
```

This command will:
 - Compile all required binaries.
 - Build Docker images for the related AI Agent workloads.

## Test

We will run SPEAR in local mode and use some Test procedures to ,verify if the overall architecture of SPEAR is functioning correctly.
First use the following command to export environment variables:

```bash
export OPENAI_API_KEY=<YOUR_OPENAI_API_KEY>
export HUGGINGFACEHUB_API_TOKEN=<YOUR_HUGGINGFACEHUB_API_TOKEN>
export SPEAR_RPC_ADDR=<YOUR_LOCAL_SPEAR_RPC_ADDR>
```
Then run the ./test/simple_req_test.go to test spear. After the program execution is completed, you will see the  output.

```bash
go test -v ./test/simple_req_test.go
```

Example Result:

```bash
=== RUN   TestSimpleReq
time="2024-12-18T15:49:56+08:00" level=info msg="Starting docker hostcall TCP server on port 8502"
time="2024-12-18T15:50:06+08:00" level=info msg="Starting spearlet on localhost:8080"
time="2024-12-18T15:50:11+08:00" level=info msg="Using transform registry chat_with_tools"
time="2024-12-18T15:50:11+08:00" level=info msg="Using model gpt-4o"
time="2024-12-18T15:50:11+08:00" level=info msg="Found 1 endpoints for gpt-4o: [{openai-toolchat gpt-4o https://api.chatanywhere.tech/v1 ******** /chat/completions}]"
time="2024-12-18T15:50:11+08:00" level=info msg="Sending request to https://api.chatanywhere.tech/v1/chat/completions"
time="2024-12-18T15:50:12+08:00" level=info msg="Reason: stop"
time="2024-12-18T15:50:12+08:00" level=info msg="Using transform registry embeddings"
time="2024-12-18T15:50:12+08:00" level=info msg="Found 1 endpoints for text-embedding-ada-002: [{openai-embed text-embedding-ada-002 https://api.chatanywhere.tech/v1 ******** /embeddings}]"
time="2024-12-18T15:50:12+08:00" level=info msg="STDERR[task-dummy-9434]:\x1b[0;31mtime=\"2024-12-18T07:50:12Z\" level=info msg=\"Response: [{map[role:system] Hello, how can I help you?} {map[role:user] I need help with my computer} {map[reason:stop role:assistant] Of course! I'd be happy to help. What specific issue are you experiencing with your computer?}]\"\x1b[0m"
time="2024-12-18T15:50:12+08:00" level=info msg="Sending request to https://api.chatanywhere.tech/v1/embeddings"
time="2024-12-18T15:50:13+08:00" level=info msg="VectorStoreCreate Request: {vdb-1734508213616043466 4}"
time="2024-12-18T15:50:13+08:00" level=info msg="Collections: []"
time="2024-12-18T15:50:13+08:00" level=info msg="Creating vector store with name vdb-1734508213616043466"
time="2024-12-18T15:50:13+08:00" level=info msg="VectorStoreInsert Request: {\"data\":\"dGVzdCBkYXRh\",\"vector\":[0.05,0.61,0.76,0.74],\"vid\":0}"
time="2024-12-18T15:50:13+08:00" level=info msg="Inserting vector into vector store with id 0"
time="2024-12-18T15:50:13+08:00" level=info msg="STDERR[task-dummy-9434]:\x1b[0;31mtime=\"2024-12-18T07:50:13Z\" level=info msg=\"Response: &{2.0 map[vid:0] <nil> 3}\"\x1b[0m"
time="2024-12-18T15:50:13+08:00" level=info msg="Upsert operation info: operation_id:0  status:Acknowledged"
time="2024-12-18T15:50:13+08:00" level=info msg="VectorStoreInsert Request: {\"data\":\"dGVzdCBkYXRh\",\"vector\":[0.19,0.81,0.75,0.11],\"vid\":0}"
time="2024-12-18T15:50:13+08:00" level=info msg="Inserting vector into vector store with id 0"
time="2024-12-18T15:50:13+08:00" level=info msg="Upsert operation info: operation_id:1  status:Acknowledged"
time="2024-12-18T15:50:13+08:00" level=info msg="VectorStoreInsert Request: {\"data\":\"dGVzdCBkYXRh\",\"vector\":[0.36,0.55,0.47,0.94],\"vid\":0}"
time="2024-12-18T15:50:13+08:00" level=info msg="Inserting vector into vector store with id 0"
time="2024-12-18T15:50:13+08:00" level=info msg="STDERR[task-dummy-9434]:\x1b[0;31mtime=\"2024-12-18T07:50:13Z\" level=info msg=\"Response: &{2.0 map[vid:0] <nil> 4}\"\x1b[0m"
time="2024-12-18T15:50:13+08:00" level=info msg="STDERR[task-dummy-9434]:\x1b[0;31mtime=\"2024-12-18T07:50:13Z\" level=info msg=\"Response: &{2.0 map[vid:0] <nil> 5}\"\x1b[0m"
time="2024-12-18T15:50:13+08:00" level=info msg="Upsert operation info: operation_id:2  status:Acknowledged"
time="2024-12-18T15:50:13+08:00" level=info msg="VectorStoreInsert Request: {\"data\":\"dGVzdCBkYXRh\",\"vector\":[0.18,0.01,0.85,0.8],\"vid\":0}"
time="2024-12-18T15:50:13+08:00" level=info msg="Inserting vector into vector store with id 0"
time="2024-12-18T15:50:13+08:00" level=info msg="Upsert operation info: operation_id:3  status:Acknowledged"
time="2024-12-18T15:50:13+08:00" level=info msg="VectorStoreInsert Request: {\"data\":\"dGVzdCBkYXRh\",\"vector\":[0.24,0.18,0.22,0.44],\"vid\":0}"
time="2024-12-18T15:50:13+08:00" level=info msg="Inserting vector into vector store with id 0"
time="2024-12-18T15:50:13+08:00" level=info msg="Upsert operation info: operation_id:4  status:Acknowledged"
time="2024-12-18T15:50:13+08:00" level=info msg="VectorStoreInsert Request: {\"data\":\"dGVzdCBkYXRh\",\"vector\":[0.35,0.08,0.11,0.44],\"vid\":0}"
time="2024-12-18T15:50:13+08:00" level=info msg="Inserting vector into vector store with id 0"
time="2024-12-18T15:50:13+08:00" level=info msg="Upsert operation info: operation_id:5  status:Acknowledged"
time="2024-12-18T15:50:13+08:00" level=info msg="VectorStoreSearch Request: {\"limit\":1,\"vector\":[0.2,0.1,0.9,0.7],\"vid\":0}"
time="2024-12-18T15:50:13+08:00" level=info msg="Searching vector in vector store with vid 0 and vector [0.2 0.1 0.9 0.7]"
time="2024-12-18T15:50:13+08:00" level=info msg="STDERR[task-dummy-9434]:\x1b[0;31mtime=\"2024-12-18T07:50:13Z\" level=info msg=\"Response: &{2.0 map[vid:0] <nil> 6}\"\x1b[0m"
time="2024-12-18T15:50:13+08:00" level=info msg="STDERR[task-dummy-9434]:\x1b[0;31mtime=\"2024-12-18T07:50:13Z\" level=info msg=\"Response: &{2.0 map[vid:0] <nil> 7}\"\x1b[0m"
time="2024-12-18T15:50:13+08:00" level=info msg="STDERR[task-dummy-9434]:\x1b[0;31mtime=\"2024-12-18T07:50:13Z\" level=info msg=\"Response: &{2.0 map[vid:0] <nil> 8}\"\x1b[0m"
time="2024-12-18T15:50:13+08:00" level=info msg="STDERR[task-dummy-9434]:\x1b[0;31mtime=\"2024-12-18T07:50:13Z\" level=info msg=\"Response: &{2.0 map[vid:0] <nil> 9}\"\x1b[0m"
time="2024-12-18T15:50:13+08:00" level=info msg="Vector is nil: id:{num:4}  score:0.99248314  version:3"
time="2024-12-18T15:50:13+08:00" level=info msg="Search result: [0xc0009007e0]"
time="2024-12-18T15:50:13+08:00" level=info msg="STDERR[task-dummy-9434]:\x1b[0;31mtime=\"2024-12-18T07:50:13Z\" level=info msg=\"Response: &{2.0 map[entries:[map[data:PG5pbD4= vector:<nil>]] vid:0] <nil> 10}\"\x1b[0m"
time="2024-12-18T15:50:13+08:00" level=info msg="VectorStoreDelete Request: {0}"
time="2024-12-18T15:50:13+08:00" level=info msg="Deleting vector store with id 0"
time="2024-12-18T15:50:13+08:00" level=info msg="STDERR[task-dummy-9434]:\x1b[0;31mtime=\"2024-12-18T07:50:13Z\" level=info msg=\"Response: &{2.0 map[vid:0] <nil> 11}\"\x1b[0m"
time="2024-12-18T15:50:13+08:00" level=info msg="Connection closed for task task-dummy-9434"
Response: null
time="2024-12-18T15:50:14+08:00" level=info msg="Server closed"
time="2024-12-18T15:50:14+08:00" level=warning msg="Error response from daemon: No such container: 7002fe074748939e8029dd228dc199f86944f63f982fd5cb7401a886cc4166c0"
--- PASS: TestSimpleReq (27.97s)
=== RUN   TestLocalDummy
time="2024-12-18T15:50:14+08:00" level=warning msg="task runtime already registered: 1"
time="2024-12-18T15:50:14+08:00" level=info msg="Starting docker hostcall TCP server on port 8536"
time="2024-12-18T15:50:23+08:00" level=info msg="Using transform registry chat_with_tools"
time="2024-12-18T15:50:23+08:00" level=info msg="Using model gpt-4o"
time="2024-12-18T15:50:23+08:00" level=info msg="Found 1 endpoints for gpt-4o: [{openai-toolchat gpt-4o https://api.chatanywhere.tech/v1 ******** /chat/completions}]"
time="2024-12-18T15:50:23+08:00" level=info msg="Sending request to https://api.chatanywhere.tech/v1/chat/completions"
time="2024-12-18T15:50:24+08:00" level=info msg="Reason: stop"
time="2024-12-18T15:50:24+08:00" level=info msg="Using transform registry embeddings"
time="2024-12-18T15:50:24+08:00" level=info msg="Found 1 endpoints for text-embedding-ada-002: [{openai-embed text-embedding-ada-002 https://api.chatanywhere.tech/v1 ******** /embeddings}]"
time="2024-12-18T15:50:24+08:00" level=info msg="Sending request to https://api.chatanywhere.tech/v1/embeddings"
time="2024-12-18T15:50:24+08:00" level=info msg="STDERR[task-dummy-107]:\x1b[0;31mtime=\"2024-12-18T07:50:24Z\" level=info msg=\"Response: [{map[role:system] Hello, how can I help you?} {map[role:user] I need help with my computer} {map[reason:stop role:assistant] Of course! I'd be happy to help. Could you please provide more details about the issue you're experiencing with your computer?}]\"\x1b[0m"
time="2024-12-18T15:50:25+08:00" level=info msg="VectorStoreCreate Request: {vdb-1734508225269885209 4}"
time="2024-12-18T15:50:25+08:00" level=info msg="Collections: []"
time="2024-12-18T15:50:25+08:00" level=info msg="Creating vector store with name vdb-1734508225269885209"
time="2024-12-18T15:50:25+08:00" level=info msg="VectorStoreInsert Request: {\"data\":\"dGVzdCBkYXRh\",\"vector\":[0.05,0.61,0.76,0.74],\"vid\":0}"
time="2024-12-18T15:50:25+08:00" level=info msg="Inserting vector into vector store with id 0"
time="2024-12-18T15:50:25+08:00" level=info msg="STDERR[task-dummy-107]:\x1b[0;31mtime=\"2024-12-18T07:50:25Z\" level=info msg=\"Response: &{2.0 map[vid:0] <nil> 3}\"\x1b[0m"
time="2024-12-18T15:50:25+08:00" level=info msg="Upsert operation info: operation_id:0  status:Acknowledged"
time="2024-12-18T15:50:25+08:00" level=info msg="VectorStoreInsert Request: {\"data\":\"dGVzdCBkYXRh\",\"vector\":[0.19,0.81,0.75,0.11],\"vid\":0}"
time="2024-12-18T15:50:25+08:00" level=info msg="Inserting vector into vector store with id 0"
time="2024-12-18T15:50:25+08:00" level=info msg="STDERR[task-dummy-107]:\x1b[0;31mtime=\"2024-12-18T07:50:25Z\" level=info msg=\"Response: &{2.0 map[vid:0] <nil> 4}\"\x1b[0m"
time="2024-12-18T15:50:25+08:00" level=info msg="Upsert operation info: operation_id:1  status:Acknowledged"
time="2024-12-18T15:50:25+08:00" level=info msg="STDERR[task-dummy-107]:\x1b[0;31mtime=\"2024-12-18T07:50:25Z\" level=info msg=\"Response: &{2.0 map[vid:0] <nil> 5}\"\x1b[0m"
time="2024-12-18T15:50:25+08:00" level=info msg="VectorStoreInsert Request: {\"data\":\"dGVzdCBkYXRh\",\"vector\":[0.36,0.55,0.47,0.94],\"vid\":0}"
time="2024-12-18T15:50:25+08:00" level=info msg="Inserting vector into vector store with id 0"
time="2024-12-18T15:50:25+08:00" level=info msg="Upsert operation info: operation_id:2  status:Acknowledged"
time="2024-12-18T15:50:25+08:00" level=info msg="VectorStoreInsert Request: {\"data\":\"dGVzdCBkYXRh\",\"vector\":[0.18,0.01,0.85,0.8],\"vid\":0}"
time="2024-12-18T15:50:25+08:00" level=info msg="STDERR[task-dummy-107]:\x1b[0;31mtime=\"2024-12-18T07:50:25Z\" level=info msg=\"Response: &{2.0 map[vid:0] <nil> 6}\"\x1b[0m"
time="2024-12-18T15:50:25+08:00" level=info msg="Inserting vector into vector store with id 0"
time="2024-12-18T15:50:25+08:00" level=info msg="Upsert operation info: operation_id:3  status:Acknowledged"
time="2024-12-18T15:50:25+08:00" level=info msg="VectorStoreInsert Request: {\"data\":\"dGVzdCBkYXRh\",\"vector\":[0.24,0.18,0.22,0.44],\"vid\":0}"
time="2024-12-18T15:50:25+08:00" level=info msg="Inserting vector into vector store with id 0"
time="2024-12-18T15:50:25+08:00" level=info msg="STDERR[task-dummy-107]:\x1b[0;31mtime=\"2024-12-18T07:50:25Z\" level=info msg=\"Response: &{2.0 map[vid:0] <nil> 7}\"\x1b[0m"
time="2024-12-18T15:50:25+08:00" level=info msg="Upsert operation info: operation_id:4  status:Acknowledged"
time="2024-12-18T15:50:25+08:00" level=info msg="VectorStoreInsert Request: {\"data\":\"dGVzdCBkYXRh\",\"vector\":[0.35,0.08,0.11,0.44],\"vid\":0}"
time="2024-12-18T15:50:25+08:00" level=info msg="Inserting vector into vector store with id 0"
time="2024-12-18T15:50:25+08:00" level=info msg="STDERR[task-dummy-107]:\x1b[0;31mtime=\"2024-12-18T07:50:25Z\" level=info msg=\"Response: &{2.0 map[vid:0] <nil> 8}\"\x1b[0m"
time="2024-12-18T15:50:25+08:00" level=info msg="Upsert operation info: operation_id:5  status:Acknowledged"
time="2024-12-18T15:50:25+08:00" level=info msg="VectorStoreSearch Request: {\"limit\":1,\"vector\":[0.2,0.1,0.9,0.7],\"vid\":0}"
time="2024-12-18T15:50:25+08:00" level=info msg="Searching vector in vector store with vid 0 and vector [0.2 0.1 0.9 0.7]"
time="2024-12-18T15:50:25+08:00" level=info msg="STDERR[task-dummy-107]:\x1b[0;31mtime=\"2024-12-18T07:50:25Z\" level=info msg=\"Response: &{2.0 map[vid:0] <nil> 9}\"\x1b[0m"
time="2024-12-18T15:50:25+08:00" level=info msg="Vector is nil: id:{num:4}  score:0.99248314  version:3"
time="2024-12-18T15:50:25+08:00" level=info msg="Search result: [0xc0006c25a0]"
time="2024-12-18T15:50:25+08:00" level=info msg="VectorStoreDelete Request: {0}"
time="2024-12-18T15:50:25+08:00" level=info msg="Deleting vector store with id 0"
time="2024-12-18T15:50:25+08:00" level=info msg="STDERR[task-dummy-107]:\x1b[0;31mtime=\"2024-12-18T07:50:25Z\" level=info msg=\"Response: &{2.0 map[entries:[map[data:PG5pbD4= vector:<nil>]] vid:0] <nil> 10}\"\x1b[0m"
time="2024-12-18T15:50:25+08:00" level=info msg="STDERR[task-dummy-107]:\x1b[0;31mtime=\"2024-12-18T07:50:25Z\" level=info msg=\"Response: &{2.0 map[vid:0] <nil> 11}\"\x1b[0m"
time="2024-12-18T15:50:25+08:00" level=info msg="Connection closed for task task-dummy-107"
    simple_req_test.go:101: Workload execution result: null
--- PASS: TestLocalDummy (11.58s)
=== RUN   TestLocalPydummy
time="2024-12-18T15:50:25+08:00" level=warning msg="task runtime already registered: 1"
time="2024-12-18T15:50:25+08:00" level=info msg="Starting docker hostcall TCP server on port 8440"
time="2024-12-18T15:50:37+08:00" level=info msg="STDERR[task-pydummy-2969]:\x1b[0;31m2024-12-18 07:50:37,632 - __main__ - INFO - Testing model: text-embedding-ada-002\x1b[0m"
time="2024-12-18T15:50:37+08:00" level=info msg="Using transform registry embeddings"
time="2024-12-18T15:50:37+08:00" level=info msg="Found 1 endpoints for text-embedding-ada-002: [{openai-embed text-embedding-ada-002 https://api.chatanywhere.tech/v1 ******** /embeddings}]"
time="2024-12-18T15:50:37+08:00" level=info msg="Sending request to https://api.chatanywhere.tech/v1/embeddings"
time="2024-12-18T15:50:38+08:00" level=info msg="STDERR[task-pydummy-2969]:\x1b[0;31m2024-12-18 07:50:38,564 - __main__ - INFO - Response Len: 4\x1b[0m"
time="2024-12-18T15:50:38+08:00" level=info msg="STDERR[task-pydummy-2969]:\x1b[0;31m2024-12-18 07:50:38,570 - __main__ - INFO - Testing model: bge-large-en-v1.5\x1b[0m"
time="2024-12-18T15:50:38+08:00" level=info msg="Using transform registry embeddings"
time="2024-12-18T15:50:38+08:00" level=info msg="Sending request to https://api-inference.huggingface.co/models/BAAI/bge-large-en-v1.5"
time="2024-12-18T15:50:42+08:00" level=warning msg="Model is not ready yet: map[error:Model BAAI/bge-large-en-v1.5 is currently loading estimated_time:53.62286376953125]"
time="2024-12-18T15:50:42+08:00" level=info msg="STDERR[task-pydummy-2969]:\x1b[0;31m2024-12-18 07:50:42,266 - __main__ - INFO - Response Len: 4\x1b[0m"
time="2024-12-18T15:50:42+08:00" level=info msg="Connection closed for task task-pydummy-2969"
    simple_req_test.go:115: Workload execution result: null
--- PASS: TestLocalPydummy (16.77s)
PASS
ok      command-line-arguments  56.330s
```


