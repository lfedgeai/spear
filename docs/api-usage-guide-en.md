# SPEAR Node Service API Usage Guide

## Overview

SPEAR Node Service provides RESTful API and gRPC interfaces for node and task management. This document focuses on the correct usage of HTTP API and common error solutions.

## Basic Information

- **HTTP Gateway Address**: `http://localhost:8080`
- **gRPC Service Address**: `localhost:50051`
- **Swagger UI**: `http://localhost:8080/swagger-ui/`
- **OpenAPI Specification**: `http://localhost:8080/api/openapi.json`

## Node Management API

### 1. Register Node

**Endpoint**: `POST /api/v1/nodes`

**Correct Request Format**:
```json
{
  "ip_address": "127.0.0.1",
  "port": 8081,
  "http_port": 8080,
  "metadata": {
    "region": "us-west-1",
    "zone": "a",
    "environment": "production"
  }
}
```

**Field Descriptions**:
- `ip_address` (required): Node IP address, string type
- `port` (required): Node port number, integer type
- `http_port` (optional): Node HTTP gateway port (used for HTTP routing / stream proxy), integer type
- `metadata` (optional): Additional metadata, key-value object

**Success Response**:
```json
{
  "message": "Node registered successfully",
  "node_uuid": "93f9a7ca-e033-4bb7-8b5a-c0899f9a52b8",
  "success": true
}
```

**Common Errors**:

1. **422 Unprocessable Entity - Missing Required Field**
   ```
   Failed to deserialize the JSON body into the target type: missing field `ip_address`
   ```
   **Solution**: Ensure `ip_address` and `port` fields are included

2. **Incorrect Field Names**
   ```bash
   # ❌ Wrong - using address field
   {
     "address": "127.0.0.1:8081"
   }
   
   # ✅ Correct - using ip_address and port separately
   {
     "ip_address": "127.0.0.1",
     "port": 8081
   }
   ```

### 2. Get All Nodes

**Endpoint**: `GET /api/v1/nodes`

**Optional Query Parameters**:
- `status`: Filter by status (active, inactive)

**Examples**:
```bash
curl http://localhost:8080/api/v1/nodes
curl http://localhost:8080/api/v1/nodes?status=active
```

**Response**:
```json
{
  "nodes": [
    {
      "uuid": "93f9a7ca-e033-4bb7-8b5a-c0899f9a52b8",
      "ip_address": "127.0.0.1",
      "port": 8081,
      "http_port": 8080,
      "status": "active",
      "last_heartbeat": 1757275996,
      "registered_at": 1757275996,
      "metadata": {
        "region": "us-west-1",
        "zone": "a"
      }
    }
  ],
  "success": true
}
```

### 3. Get Specific Node

**Endpoint**: `GET /api/v1/nodes/{uuid}`

**Example**:
```bash
curl http://localhost:8080/api/v1/nodes/93f9a7ca-e033-4bb7-8b5a-c0899f9a52b8
```

### 4. Update Node

**Endpoint**: `PUT /api/v1/nodes/{uuid}`

**Request Format**:
```json
{
  "ip_address": "192.168.1.100",
  "port": 8082,
  "http_port": 8080,
  "status": "active",
  "metadata": {
    "region": "us-east-1",
    "zone": "b"
  }
}
```

## Execution Stream API

These endpoints are used to establish a bidirectional WebSocket stream for a specific execution.

### 1. Create Stream Session (Recommended)

**Endpoint**: `POST /api/v1/executions/{execution_id}/streams/session`

**Usage**: Returns a short-lived token and a `ws_url` that clients can directly connect to.

**Response Example**:
```json
{
  "execution_id": "exec-123",
  "token": "<short-lived-token>",
  "ws_url": "ws://localhost:8080/api/v1/executions/exec-123/streams/ws?token=<short-lived-token>",
  "expires_in_ms": 60000
}
```

### 2. WebSocket Stream (Data Plane)

**Endpoint**: `GET /api/v1/executions/{execution_id}/streams/ws?token=...`

**Notes**:
- Clients SHOULD call `streams/session` first and then connect using the returned `ws_url`.
- Each WebSocket binary message is one SSF frame.

### 5. Delete Node

**Endpoint**: `DELETE /api/v1/nodes/{uuid}`

**Example**:
```bash
curl -X DELETE http://localhost:8080/api/v1/nodes/93f9a7ca-e033-4bb7-8b5a-c0899f9a52b8
```

### 6. Node Heartbeat

**Endpoint**: `POST /api/v1/nodes/{uuid}/heartbeat`

**Request Format**:
```json
{
  "health_info": {
    "cpu_usage": "45.2",
    "memory_usage": "67.8",
    "status": "healthy"
  }
}
```

## Node Resource Management API

### Update Node Resource Information

**Endpoint**: `PUT /api/v1/nodes/{uuid}/resource`

**Request Format**:
```json
{
  "cpu_usage_percent": 45.2,
  "memory_usage_percent": 67.8,
  "total_memory_bytes": 8589934592,
  "used_memory_bytes": 5825830912,
  "available_memory_bytes": 2764103680,
  "disk_usage_percent": 78.5,
  "total_disk_bytes": 1099511627776,
  "used_disk_bytes": 863597383680,
  "network_rx_bytes_per_sec": 1048576,
  "network_tx_bytes_per_sec": 524288,
  "load_average_1m": 1.25,
  "load_average_5m": 1.15,
  "load_average_15m": 1.05,
  "resource_metadata": {
    "gpu_count": "2",
    "gpu_memory": "24GB"
  }
}
```

## Task Management API

### 1. Register Task

**Endpoint**: `POST /api/v1/tasks`

**Request Format**:
```json
{
  "name": "image-processing-task",
  "description": "Process images using AI models",
  "desired_replicas": 2,
  "scheduling_strategy": "spread",
  "endpoint": "http://127.0.0.1:8081/process",
  "version": "1.0.0",
  "capabilities": ["image-processing", "ai-inference"],
  "priority": "high",
  "executable": {
    "type": "wasm",
    "uri": "smsfile://<file_id>",
    "name": "hello.wasm",
    "args": [],
    "env": {}
  }
}
```

**Executable Descriptor**:
- `type`: One of `binary|script|container|wasm|process`
- `uri`: Canonical URI (e.g. `smsfile://<id>`, `http://...`, `docker://image:tag`)
- `name`: Optional local alias
- `checksum_sha256`: Optional integrity checksum
- `args` and `env`: Default arguments and environment variables

Note: For `type=wasm`, Spearlet runtime strictly requires a valid WASM binary at instance creation time. If the downloaded/passed bytes are not a valid WASM module, instance creation fails with `InvalidConfiguration`.

### 2. Get All Tasks

**Endpoint**: `GET /api/v1/tasks`

**Optional Query Parameters**:
- `status`: Filter by status
- `priority`: Filter by priority

## Cluster Statistics API

### Get Cluster Statistics

**Endpoint**: `GET /api/v1/cluster/stats`

**Response**:
```json
{
  "stats": {
    "total_nodes": 5,
    "active_nodes": 4,
    "inactive_nodes": 1,
    "unhealthy_nodes": 0,
    "nodes_with_resources": 3,
    "average_cpu_usage": 45.6,
    "average_memory_usage": 67.2,
    "total_memory_bytes": 42949672960,
    "total_used_memory_bytes": 28858370048,
    "high_load_nodes": 1
  },
  "success": true
}
```

## Health Check

**Endpoint**: `GET /health`

**Response**:
```json
{
  "service": "sms",
  "status": "healthy",
  "timestamp": "2025-09-07T20:11:48.287757+00:00"
}
```

## Common Errors and Solutions

### 1. 422 Unprocessable Entity

**Cause**: Incorrect request body format or missing required fields

**Solution**:
- Check if JSON format is correct
- Ensure all required fields are included
- Verify field types are correct

### 2. 404 Not Found

**Cause**: Requested resource does not exist

**Solution**:
- Check if UUID is correct
- Confirm resource has been created

### 3. 500 Internal Server Error

**Cause**: Internal server error

**Solution**:
- Check server logs
- Confirm gRPC service is running properly

## Complete Examples Using curl

```bash
# 1. Check service health
curl http://localhost:8080/health

# 2. Register a new node
NODE_UUID=$(curl -s -X POST http://localhost:8080/api/v1/nodes \
  -H "Content-Type: application/json" \
  -d '{
    "ip_address": "127.0.0.1",
    "port": 8081,
    "metadata": {"region": "us-west-1"}
  }' | jq -r '.node_uuid')

echo "Created node: $NODE_UUID"

# 3. Get all nodes
curl -s http://localhost:8080/api/v1/nodes | jq .

# 4. Update node resource information
curl -X PUT http://localhost:8080/api/v1/nodes/$NODE_UUID/resource \
  -H "Content-Type: application/json" \
  -d '{
    "cpu_usage_percent": 45.2,
    "memory_usage_percent": 67.8,
    "total_memory_bytes": 8589934592,
    "used_memory_bytes": 5825830912
  }'

# 5. Send heartbeat
curl -X POST http://localhost:8080/api/v1/nodes/$NODE_UUID/heartbeat \
  -H "Content-Type: application/json" \
  -d '{
    "health_info": {"status": "healthy", "uptime": "3600"}
  }'

# 6. Register task
curl -X POST http://localhost:8080/api/v1/tasks \
  -H "Content-Type: application/json" \
  -d '{
    "name": "test-task",
    "description": "Test task",
    "desired_replicas": 1,
    "scheduling_strategy": "spread",
    "endpoint": "http://127.0.0.1:8081/test",
    "version": "1.0.0",
    "capabilities": ["test"],
    "priority": "medium"
  }'

# 7. Get cluster statistics
curl -s http://localhost:8080/api/v1/cluster/stats | jq .
```

## Using Swagger UI

1. Open browser and visit: `http://localhost:8080/swagger-ui/`
2. In Swagger UI you can:
   - View all available API endpoints
   - Test API calls
   - View request and response formats
   - Download OpenAPI specification

## Important Notes

1. **Field Naming**: API uses `ip_address` and `port` fields, not `address`
2. **Data Types**: Ensure `port` is integer type, not string
3. **UUID Format**: Node and task UUIDs must be valid UUID v4 format
4. **Timestamps**: API returns Unix timestamps (seconds)
5. **Metadata**: `metadata` field must be a string key-value object

## Related Documentation

- [gRPC Transport Error Troubleshooting](grpc-transport-error-troubleshooting-en.md)
- [CLI Configuration Guide](cli-configuration-en.md)
- [Service Configuration](../config.toml)
