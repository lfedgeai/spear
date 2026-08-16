# Task List Filtering Refactor

## Overview

This document explains the current task list filtering behavior in SMS, including the HTTP query parameters, the protobuf request encoding, and the server-side filtering implementation.

The key design goal is to support optional filters while remaining protobuf-compatible. The implementation uses an integer sentinel (`-1`) to represent "no filter".

## API Surface

### HTTP

- Endpoint: `GET /api/v1/tasks`
- Optional query parameters:
  - `status`: one of `unknown|registered|created|active|inactive|unregistered`
  - `priority`: one of `unknown|low|normal|high|urgent`
  - `limit` (default 100)
  - `offset` (default 0)

Example:

```text
GET /api/v1/tasks?status=active&priority=high&limit=50&offset=0
```

Implementation: `src/sms/handlers/task.rs:252`.

### gRPC (SMS)

- RPC: `ListTasks(ListTasksRequest) returns (ListTasksResponse)`
- Proto: `proto/sms/task.proto:106`

The proto fields `status_filter` and `priority_filter` are enums, but are encoded as integers in generated code. This implementation treats negative values as "no filter".

## Encoding: `FilterState` and `NO_FILTER`

`src/sms/types.rs:7` defines:

- `NO_FILTER: i32 = -1`
- `FilterState::{None, Value(i32)}` and `FilterState::to_i32()`

HTTP handlers convert optional query params into `FilterState`, then into `i32` for protobuf request compatibility:

- If query param is missing or invalid: `FilterState::None -> -1`
- If query param is present and valid: `FilterState::Value(enum as i32)`

Implementation: `src/sms/handlers/task.rs:259`.

## Server-side Filtering

### SMS Service Handler

`src/sms/service.rs:633` converts request fields into optional filters:

- `status_filter < 0` => `None`
- `priority_filter < 0` => `None`
- `limit <= 0` => `None`
- `offset < 0` => `None`

Then it calls `TaskService::list_tasks_with_filters(...)`.

### TaskService

`src/sms/services/task_service.rs:57` applies filters:

- If `status_filter` is set and `>= 0`, tasks must match `task.status`.
- If `priority_filter` is set and `>= 0`, tasks must match `task.priority`.

Pagination:

- `offset` defaults to 0
- `limit` defaults to 100
- If `offset` exceeds the filtered length, returns an empty list.

## Notes

- `total_count` in `ListTasksResponse` is the total number of tasks before filtering (used for pagination UI). See `src/sms/service.rs:667`.
- Web Admin also uses the sentinel `-1` for "no filter" when listing tasks. See `src/sms/web_admin.rs:269`.

Version: v1.0 (validated against current code on 2025-12-16).
