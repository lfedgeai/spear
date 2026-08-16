use axum::{extract::Path, http::StatusCode, response::Html, Json};
use serde_json::json;

/// OpenAPI specification / OpenAPI规范
pub async fn openapi_spec() -> Json<serde_json::Value> {
    Json(json!({
        "openapi": "3.0.0",
        "info": {
            "title": "SPEAR Metadata Server API",
            "version": "1.0.0",
            "description": "SPEAR Metadata Server API for managing compute nodes and resources"
        },
        "servers": [
            {
                "url": "http://localhost:8080",
                "description": "Local development server"
            }
        ],
        "paths": {
            "/api/v1/nodes": {
                "get": {
                    "summary": "List all nodes",
                    "parameters": [
                        {
                            "name": "status",
                            "in": "query",
                            "schema": {
                                "type": "string",
                                "enum": ["active", "inactive", "unhealthy", "decommissioning"]
                            }
                        }
                    ],
                    "responses": {
                        "200": {
                            "description": "List of nodes"
                        }
                    }
                },
                "post": {
                    "summary": "Register a new node",
                    "requestBody": {
                        "required": true,
                        "content": {
                            "application/json": {
                                "schema": {
                                    "type": "object",
                                    "required": ["ip_address", "port"],
                                    "properties": {
                                        "ip_address": {"type": "string"},
                                        "port": {"type": "integer"},
                                        "metadata": {"type": "object"}
                                    }
                                }
                            }
                        }
                    },
                    "responses": {
                        "200": {
                            "description": "Node registration result"
                        }
                    }
                }
            },
            "/api/v1/nodes/{uuid}": {
                "get": {
                    "summary": "Get a specific node",
                    "parameters": [
                        {
                            "name": "uuid",
                            "in": "path",
                            "required": true,
                            "schema": {"type": "string"}
                        }
                    ],
                    "responses": {
                        "200": {
                            "description": "Node information"
                        }
                    }
                },
                "put": {
                    "summary": "Update a node",
                    "parameters": [
                        {
                            "name": "uuid",
                            "in": "path",
                            "required": true,
                            "schema": {"type": "string"}
                        }
                    ],
                    "requestBody": {
                        "required": true,
                        "content": {
                            "application/json": {
                                "schema": {
                                    "type": "object",
                                    "properties": {
                                        "ip_address": {"type": "string"},
                                        "port": {"type": "integer"},
                                        "status": {"type": "string"},
                                        "metadata": {"type": "object"}
                                    }
                                }
                            }
                        }
                    },
                    "responses": {
                        "200": {
                            "description": "Node update result"
                        }
                    }
                },
                "delete": {
                    "summary": "Delete a node",
                    "parameters": [
                        {
                            "name": "uuid",
                            "in": "path",
                            "required": true,
                            "schema": {"type": "string"}
                        }
                    ],
                    "responses": {
                        "200": {
                            "description": "Node deletion result"
                        }
                    }
                }
            },
            "/api/v1/nodes/{uuid}/heartbeat": {
                "post": {
                    "summary": "Send heartbeat for a node",
                    "parameters": [
                        {
                            "name": "uuid",
                            "in": "path",
                            "required": true,
                            "schema": {"type": "string"}
                        }
                    ],
                    "requestBody": {
                        "required": false,
                        "content": {
                            "application/json": {
                                "schema": {
                                    "type": "object",
                                    "properties": {
                                        "health_info": {"type": "object"}
                                    }
                                }
                            }
                        }
                    },
                    "responses": {
                        "200": {
                            "description": "Heartbeat result"
                        }
                    }
                }
            },
            "/api/v1/nodes/{uuid}/resource": {
                "put": {
                    "summary": "Update node resource information",
                    "parameters": [
                        {
                            "name": "uuid",
                            "in": "path",
                            "required": true,
                            "schema": {"type": "string"}
                        }
                    ],
                    "requestBody": {
                        "required": true,
                        "content": {
                            "application/json": {
                                "schema": {
                                    "type": "object",
                                    "properties": {
                                        "cpu_usage_percent": {"type": "number", "format": "float"},
                                        "memory_usage_percent": {"type": "number", "format": "float"},
                                        "total_memory_bytes": {"type": "integer", "format": "int64"},
                                        "used_memory_bytes": {"type": "integer", "format": "int64"},
                                        "available_memory_bytes": {"type": "integer", "format": "int64"},
                                        "disk_usage_percent": {"type": "number", "format": "float"},
                                        "total_disk_bytes": {"type": "integer", "format": "int64"},
                                        "used_disk_bytes": {"type": "integer", "format": "int64"},
                                        "network_rx_bytes_per_sec": {"type": "integer", "format": "int64"},
                                        "network_tx_bytes_per_sec": {"type": "integer", "format": "int64"},
                                        "load_average_1m": {"type": "number", "format": "float"},
                                        "load_average_5m": {"type": "number", "format": "float"},
                                        "load_average_15m": {"type": "number", "format": "float"},
                                        "resource_metadata": {"type": "object"}
                                    }
                                }
                            }
                        }
                    },
                    "responses": {
                        "200": {
                            "description": "Resource update result"
                        }
                    }
                },
                "get": {
                    "summary": "Get node resource information",
                    "parameters": [
                        {
                            "name": "uuid",
                            "in": "path",
                            "required": true,
                            "schema": {"type": "string"}
                        }
                    ],
                    "responses": {
                        "200": {
                            "description": "Node resource information"
                        },
                        "404": {
                            "description": "Resource not found"
                        }
                    }
                }
            },
            "/api/v1/resources": {
                "get": {
                    "summary": "List node resources",
                    "parameters": [
                        {
                            "name": "node_uuids",
                            "in": "query",
                            "schema": {"type": "string"},
                            "description": "Comma-separated list of node UUIDs to filter by"
                        }
                    ],
                    "responses": {
                        "200": {
                            "description": "List of node resources"
                        }
                    }
                }
            },
            "/api/v1/nodes/{uuid}/with-resource": {
                "get": {
                    "summary": "Get node with its resource information",
                    "parameters": [
                        {
                            "name": "uuid",
                            "in": "path",
                            "required": true,
                            "schema": {"type": "string"}
                        }
                    ],
                    "responses": {
                        "200": {
                            "description": "Node with resource information"
                        },
                        "404": {
                            "description": "Node not found"
                        }
                    }
                }
            },
            "/api/v1/tasks": {
                "post": {
                    "summary": "Register a new task",
                    "requestBody": {
                        "required": true,
                        "content": {
                            "application/json": {
                                "schema": {
                                    "type": "object",
                                    "required": ["name"],
                                    "properties": {
                                        "name": {"type": "string"},
                                        "description": {"type": "string"},
                                        "priority": {"type": "string", "enum": ["low", "normal", "high", "urgent"]},
                                        "desired_replicas": {"type": "integer", "minimum": 1},
                                        "scheduling_strategy": {"type": "string", "enum": ["spread"]},
                                        "endpoint": {"type": "string"},
                                        "version": {"type": "string"},
                                        "capabilities": {"type": "array", "items": {"type": "string"}},
                                        "metadata": {"type": "object"},
                                        "config": {"type": "object"}
                                    }
                                }
                            }
                        }
                    },
                    "responses": {
                        "200": {
                            "description": "Task registration result"
                        }
                    }
                },
                "get": {
                    "summary": "List tasks",
                    "parameters": [
                        {
                            "name": "status",
                            "in": "query",
                            "schema": {
                                "type": "string",
                                "enum": ["pending", "running", "completed", "failed", "cancelled"]
                            }
                        },
                        {
                            "name": "priority",
                            "in": "query",
                            "schema": {
                                "type": "string",
                                "enum": ["low", "normal", "high", "urgent"]
                            }
                        },
                        {
                            "name": "limit",
                            "in": "query",
                            "schema": {"type": "integer"}
                        },
                        {
                            "name": "offset",
                            "in": "query",
                            "schema": {"type": "integer"}
                        }
                    ],
                    "responses": {
                        "200": {
                            "description": "List of tasks"
                        }
                    }
                }
            },
            "/api/v1/tasks/{task_id}": {
                "get": {
                    "summary": "Get a specific task",
                    "parameters": [
                        {
                            "name": "task_id",
                            "in": "path",
                            "required": true,
                            "schema": {"type": "string"}
                        }
                    ],
                    "responses": {
                        "200": {
                            "description": "Task information"
                        },
                        "404": {
                            "description": "Task not found"
                        }
                    }
                },
                "delete": {
                    "summary": "Unregister a task",
                    "parameters": [
                        {
                            "name": "task_id",
                            "in": "path",
                            "required": true,
                            "schema": {"type": "string"}
                        }
                    ],
                    "responses": {
                        "200": {
                            "description": "Task unregistration result"
                        },
                        "404": {
                            "description": "Task not found"
                        }
                    }
                }
            }
            ,
            "/api/v1/placement/invocations/place": {
                "post": {
                    "summary": "Place an invocation and return candidate nodes",
                    "requestBody": {
                        "required": true,
                        "content": {
                            "application/json": {
                                "schema": {
                                    "type": "object",
                                    "required": ["request_id", "task_id"],
                                    "properties": {
                                        "request_id": {"type": "string"},
                                        "task_id": {"type": "string"},
                                        "max_candidates": {"type": "integer"},
                                        "labels": {"type": "object"}
                                    }
                                }
                            }
                        }
                    },
                    "responses": {
                        "200": {"description": "Placement decision and candidates"}
                    }
                }
            },
            "/api/v1/placement/invocations/report-outcome": {
                "post": {
                    "summary": "Report invocation outcome for placement feedback",
                    "requestBody": {
                        "required": true,
                        "content": {
                            "application/json": {
                                "schema": {
                                    "type": "object",
                                    "required": ["decision_id", "request_id", "task_id", "node_uuid"],
                                    "properties": {
                                        "decision_id": {"type": "string"},
                                        "request_id": {"type": "string"},
                                        "task_id": {"type": "string"},
                                        "node_uuid": {"type": "string"},
                                        "outcome_class": {"type": "string"},
                                        "error_message": {"type": "string"}
                                    }
                                }
                            }
                        }
                    },
                    "responses": {
                        "200": {"description": "Outcome accepted"}
                    }
                }
            }
        }
    }))
}

/// Swagger UI main page / Swagger UI主页
pub async fn swagger_ui() -> Html<String> {
    Html(r#"
<!DOCTYPE html>
<html>
<head>
    <title>Swagger UI - SPEAR Metadata Server API Documentation</title>
    <link rel="stylesheet" type="text/css" href="https://unpkg.com/swagger-ui-dist@4.15.5/swagger-ui.css" />
    <style>
        html {
            box-sizing: border-box;
            overflow: -moz-scrollbars-vertical;
            overflow-y: scroll;
        }
        *, *:before, *:after {
            box-sizing: inherit;
        }
        body {
            margin:0;
            background: #fafafa;
        }
    </style>
</head>
<body>
    <div id="swagger-ui"></div>
    <script src="https://unpkg.com/swagger-ui-dist@4.15.5/swagger-ui-bundle.js"></script>
    <script src="https://unpkg.com/swagger-ui-dist@4.15.5/swagger-ui-standalone-preset.js"></script>
    <script>
        window.onload = function() {
            const ui = SwaggerUIBundle({
                url: '/api/openapi.json',
                dom_id: '#swagger-ui',
                deepLinking: true,
                presets: [
                    SwaggerUIBundle.presets.apis,
                    SwaggerUIStandalonePreset
                ],
                plugins: [
                    SwaggerUIBundle.plugins.DownloadUrl
                ],
                layout: "StandaloneLayout"
            });
        };
    </script>
</body>
</html>
"#.to_string())
}

/// Swagger UI static assets / Swagger UI静态资源
pub async fn swagger_ui_assets(Path(_file): Path<String>) -> Result<(), StatusCode> {
    // For now, we use CDN for Swagger UI assets
    // In production, you might want to serve these locally
    // 目前我们使用CDN提供Swagger UI资源
    // 在生产环境中，您可能希望在本地提供这些资源
    Err(StatusCode::NOT_FOUND)
}
