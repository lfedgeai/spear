# SPEAR Node Service API 使用指南

## 概述

SPEAR Node Service 提供了 RESTful API 和 gRPC 接口用于节点和任务管理。本文档重点介绍 HTTP API 的正确使用方法和常见错误解决方案。

## 基础信息

- **HTTP 网关地址**: `http://localhost:8080`
- **gRPC 服务地址**: `localhost:50051`
- **Swagger UI**: `http://localhost:8080/swagger-ui/`
- **OpenAPI 规范**: `http://localhost:8080/api/openapi.json`

## 节点管理 API

### 1. 注册节点

**端点**: `POST /api/v1/nodes`

**正确的请求格式**:
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

**字段说明**:
- `ip_address` (必需): 节点的 IP 地址，字符串类型
- `port` (必需): 节点的端口号，整数类型
- `http_port` (可选): 节点 HTTP 网关端口（用于 HTTP 路由/stream 代理），整数类型
- `metadata` (可选): 额外的元数据，键值对对象

**成功响应**:
```json
{
  "message": "Node registered successfully",
  "node_uuid": "93f9a7ca-e033-4bb7-8b5a-c0899f9a52b8",
  "success": true
}
```

**常见错误**:

1. **422 Unprocessable Entity - 缺少必需字段**
   ```
   Failed to deserialize the JSON body into the target type: missing field `ip_address`
   ```
   **解决方案**: 确保包含 `ip_address` 和 `port` 字段

2. **字段名称错误**
   ```bash
   # ❌ 错误 - 使用 address 字段
   {
     "address": "127.0.0.1:8081"
   }
   
   # ✅ 正确 - 分别使用 ip_address 和 port
   {
     "ip_address": "127.0.0.1",
     "port": 8081
   }
   ```

### 2. 获取所有节点

**端点**: `GET /api/v1/nodes`

**可选查询参数**:
- `status`: 按状态过滤 (active, inactive)

**示例**:
```bash
curl http://localhost:8080/api/v1/nodes
curl http://localhost:8080/api/v1/nodes?status=active
```

**响应**:
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

### 3. 获取特定节点

**端点**: `GET /api/v1/nodes/{uuid}`

**示例**:
```bash
curl http://localhost:8080/api/v1/nodes/93f9a7ca-e033-4bb7-8b5a-c0899f9a52b8
```

### 4. 更新节点

**端点**: `PUT /api/v1/nodes/{uuid}`

**请求格式**:
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

这些端点用于为某个 execution 建立双向 WebSocket stream。

### 1. 创建 Stream Session（推荐）

**端点**: `POST /api/v1/executions/{execution_id}/streams/session`

**用途**: 返回短期 token 和可直接连接的 `ws_url`。

**响应示例**:
```json
{
  "execution_id": "exec-123",
  "token": "<short-lived-token>",
  "ws_url": "ws://localhost:8080/api/v1/executions/exec-123/streams/ws?token=<short-lived-token>",
  "expires_in_ms": 60000
}
```

### 2. WebSocket Stream（数据面）

**端点**: `GET /api/v1/executions/{execution_id}/streams/ws?token=...`

**说明**:
- 客户端应先调用 `streams/session`，再用返回的 `ws_url` 进行连接。
- 每个 WebSocket binary message 对应一个 SSF frame。

### 5. 删除节点

**端点**: `DELETE /api/v1/nodes/{uuid}`

**示例**:
```bash
curl -X DELETE http://localhost:8080/api/v1/nodes/93f9a7ca-e033-4bb7-8b5a-c0899f9a52b8
```

### 6. 节点心跳

**端点**: `POST /api/v1/nodes/{uuid}/heartbeat`

**请求格式**:
```json
{
  "health_info": {
    "cpu_usage": "45.2",
    "memory_usage": "67.8",
    "status": "healthy"
  }
}
```

## 节点资源管理 API

### 更新节点资源信息

**端点**: `PUT /api/v1/nodes/{uuid}/resource`

**请求格式**:
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

## 任务管理 API

### 1. 注册任务

**端点**: `POST /api/v1/tasks`

**请求格式**:
```json
{
  "name": "image-processing-task",
  "description": "Process images using AI models",
  "desired_replicas": 2,
  "scheduling_strategy": "spread",
  "endpoint": "http://127.0.0.1:8081/process",
  "version": "1.0.0",
  "capabilities": ["image-processing", "ai-inference"],
  "priority": "high"
}
```

### 2. 获取所有任务

**端点**: `GET /api/v1/tasks`

**可选查询参数**:
- `status`: 按状态过滤
- `priority`: 按优先级过滤

## 集群统计 API

### 获取集群统计信息

**端点**: `GET /api/v1/cluster/stats`

**响应**:
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

## 健康检查

**端点**: `GET /health`

**响应**:
```json
{
  "service": "sms",
  "status": "healthy",
  "timestamp": "2025-09-07T20:11:48.287757+00:00"
}
```

## 常见错误和解决方案

### 1. 422 Unprocessable Entity

**原因**: 请求体格式不正确或缺少必需字段

**解决方案**:
- 检查 JSON 格式是否正确
- 确保包含所有必需字段
- 验证字段类型是否正确

### 2. 404 Not Found

**原因**: 请求的资源不存在

**解决方案**:
- 检查 UUID 是否正确
- 确认资源是否已创建

### 3. 500 Internal Server Error

**原因**: 服务器内部错误

**解决方案**:
- 检查服务器日志
- 确认 gRPC 服务是否正常运行

## 使用 curl 的完整示例

```bash
# 1. 检查服务健康状态
curl http://localhost:8080/health

# 2. 注册一个新节点
NODE_UUID=$(curl -s -X POST http://localhost:8080/api/v1/nodes \
  -H "Content-Type: application/json" \
  -d '{
    "ip_address": "127.0.0.1",
    "port": 8081,
    "metadata": {"region": "us-west-1"}
  }' | jq -r '.node_uuid')

echo "Created node: $NODE_UUID"

# 3. 获取所有节点
curl -s http://localhost:8080/api/v1/nodes | jq .

# 4. 更新节点资源信息
curl -X PUT http://localhost:8080/api/v1/nodes/$NODE_UUID/resource \
  -H "Content-Type: application/json" \
  -d '{
    "cpu_usage_percent": 45.2,
    "memory_usage_percent": 67.8,
    "total_memory_bytes": 8589934592,
    "used_memory_bytes": 5825830912
  }'

# 5. 发送心跳
curl -X POST http://localhost:8080/api/v1/nodes/$NODE_UUID/heartbeat \
  -H "Content-Type: application/json" \
  -d '{
    "health_info": {"status": "healthy", "uptime": "3600"}
  }'

# 6. 注册任务
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

# 7. 获取集群统计
curl -s http://localhost:8080/api/v1/cluster/stats | jq .
```

## 使用 Swagger UI

1. 打开浏览器访问: `http://localhost:8080/swagger-ui/`
2. 在 Swagger UI 中可以：
   - 查看所有可用的 API 端点
   - 测试 API 调用
   - 查看请求和响应格式
   - 下载 OpenAPI 规范

## 注意事项

1. **字段命名**: API 使用 `ip_address` 和 `port` 字段，不是 `address`
2. **数据类型**: 确保 `port` 是整数类型，不是字符串
3. **UUID 格式**: 节点和任务的 UUID 必须是有效的 UUID v4 格式
4. **时间戳**: API 返回的时间戳是 Unix 时间戳（秒）
5. **元数据**: `metadata` 字段必须是字符串键值对对象

## 相关文档

- [gRPC Transport Error 故障排除](grpc-transport-error-troubleshooting-zh.md)
- [CLI 配置指南](cli-configuration-zh.md)
- [服务配置文档](../config.toml)
