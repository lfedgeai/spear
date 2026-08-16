# Task API Refactoring Documentation

## Overview

This document describes the comprehensive refactoring of the Task API in the SPEAR-Next project. The refactoring simplified the task management operations from a complex lifecycle model to a straightforward registration-based model.

## Changes Made

### 1. Proto Definition Simplification

**File**: `proto/sms/task.proto`

**Before**: Complex task lifecycle with submit, stop, kill operations
**After**: Simplified registration model with register, list, get, unregister, and coordinated delete operations

Key changes:
- Removed `SubmitTaskRequest`, `StopTaskRequest`, `KillTaskRequest`
- Added `RegisterTaskRequest`, `UnregisterTaskRequest`, `DeleteTaskRequest`, and `CompleteTaskDeletionRequest`
- Simplified task states to focus on registration status
- Added the `deleting` lifecycle state for coordinated cleanup
- Updated task structure to include endpoint, version, capabilities, and config fields

### 2. Service Layer Refactoring

**File**: `src/services/task.rs`

**Changes**:
- Removed `submit_task`, `stop_task`, `kill_task` methods
- Added `register_task`, `unregister_task` methods
- Simplified task storage model
- Updated task validation logic
- Maintained `list_tasks` and `get_task` methods with updated logic

**Key Features**:
- Task registration with endpoint and capability information
- Priority-based task management
- Simplified state management (registered/unregistered)

### 3. HTTP Handlers Update

**File**: `src/http/handlers/task.rs`

**Changes**:
- Updated `RegisterTaskParams` structure
- Removed submit/stop/kill handlers
- Added unregister handler and coordinated delete handler
- Updated response structures
- Improved error handling

**API Endpoints**:
- `POST /api/v1/tasks` - Register a new task
- `GET /api/v1/tasks` - List tasks with filtering
- `GET /api/v1/tasks/:task_id` - Get task details
- `DELETE /api/v1/tasks/:task_id` - Request coordinated task deletion

### 3.1 Coordinated Delete Flow

The task delete path is now a two-phase flow:

1. `DeleteTask` marks the task as `deleting` and stops endpoint resolution.
2. SMS publishes a cancel event to the target node.
3. Spearlet stops and destroys instances that belong to the task.
4. Spearlet calls `CompleteTaskDeletion` after runtime cleanup finishes.
5. SMS finalizes the task record and removes the endpoint index.

Execution history is intentionally retained outside the task record cleanup path.

### 4. Route Configuration

**File**: `src/http/routes.rs`

**Changes**:
- Updated task management routes
- Removed redundant route definitions
- Simplified route structure

### 4.1 Web Admin Support

- Added `DELETE /admin/api/tasks/{task_id}` in SMS Web Admin.
- Added task delete actions in the task list and task detail pages.
- Web Admin now submits `reason` and `force` to the coordinated delete API.

### 4.2 Code Organization Cleanup

- Moved SMS task gRPC orchestration into `src/sms/task_rpc.rs`.
- Moved Web Admin task handlers into `src/sms/web_admin/task_admin.rs`.
- Extracted spearlet task event cursor persistence into `src/spearlet/task_event_cursor.rs`.
- Extracted spearlet SMS reporting helpers into `src/spearlet/execution/sms_reporter.rs`.
- Extracted local task runtime teardown into `src/spearlet/execution/task_runtime_cleanup.rs`.
- Extracted shared SMS task materialization into `src/spearlet/execution/task_materializer.rs`.
- Extracted shared execution completion helpers into `src/spearlet/execution/execution_finalize.rs`.
- Added `src/spearlet/execution/sms_status_adapter.rs` so spearlet-local task/instance/runtime states are projected to SMS proto statuses in one place.
- Preserved terminal execution semantics when projecting to SMS, so runtime `Cancelled` and `Timeout` now remain distinct instead of being flattened into `Failed`.
- Added `src/spearlet/execution/execution_status.rs` to centralize execution public status parsing, terminal-state checks, and spearlet proto projection.
- Removed ad hoc `"pending"` / `"running"` / `"completed"` string checks from manager and function-service flow code in favor of the shared execution status helper.
- Moved task inventory-driven lifecycle reconciliation into `src/spearlet/execution/task.rs`, so adding/removing instances now updates local `TaskStatus` consistently instead of relying on manager-side ad hoc transitions.
- Added focused task lifecycle helpers for draining and idle-retention checks, reducing status-specific branching in manager and cleanup paths.
- Added `src/spearlet/execution/task_public_status.rs` to centralize `TaskStatus ->` spearlet HTTP display status mapping.
- Removed the remaining task presentation-specific mapping logic from `src/spearlet/http_gateway.rs`, so task display status now follows the same shared-semantic cleanup pattern as execution status.
- Refactored `src/spearlet/execution/sms_reporter.rs` to separate SMS request construction from fire-and-forget RPC dispatch, reducing repeated channel/timeout/spawn boilerplate.
- Made task status/result publishing in the reporter explicitly fire-and-forget instead of exposing misleading `async` wrappers that never awaited network work.
- Removed trivial one-line forwarding wrappers from `src/spearlet/execution/manager.rs` and `src/spearlet/http_gateway.rs` where they no longer added meaningful domain semantics.
- Continued trimming manager-local one-line wrappers that only forwarded to `sms_reporter` or `scheduler`, while intentionally keeping single-line helpers that still encode task/status domain meaning.
- Removed additional cross-module trivial wrappers while scanning more Rust files, including manager-local duplicate getters, a fake-async execution status query, and SMS-side thin status/endpoint conversion helpers.
- Continued pruning trivial wrappers in `src/sms/service.rs` and `src/spearlet/http_gateway.rs`, removing unused service getters, an MCP upsert passthrough helper, and a one-line timestamp formatting wrapper where inline usage stayed equally clear.
- Removed another SMS-side trivial wrapper in `src/sms/grpc_server.rs` by inlining the `prepare()` passthrough into the two startup paths, keeping the startup log but avoiding an extra jump for a two-value tuple.
- Folded spearlet HTTP execution status rendering back into `src/spearlet/execution/execution_status.rs`, so `http_gateway.rs` no longer owns a separate proto-to-display `match` table for execution status strings.
- Split `src/spearlet/http_gateway.rs` along handler boundaries by moving execution and task HTTP handlers into `src/spearlet/http_gateway/execution_handlers.rs` and `src/spearlet/http_gateway/task_handlers.rs`, leaving the root gateway file focused on shared state, routing, websocket, object, and monitoring concerns.
- Continued the gateway split by moving object and monitoring HTTP handlers into `src/spearlet/http_gateway/object_handlers.rs` and `src/spearlet/http_gateway/monitoring_handlers.rs`, so the root gateway file now mostly serves as routing, shared state, websocket, and docs composition.
- Replaced the earlier policy-style task visibility plumbing with semantic task access helpers in `src/sms/task_semantics.rs`: control-plane reads keep using `get_task(...)`, while endpoint routing now goes through `resolve_routable_task_by_endpoint(...)`.
- Simplified spearlet task creation/mirroring paths by routing both SMS watch-event prewarm and invocation-time cache miss through manager-owned SMS sync entrypoints, instead of letting `task_events.rs` fetch task snapshots on its own.
- Tightened `TaskEventSubscriber` semantics so cursor advancement now happens only after successful task-event handling, giving create/cancel replay a clearer at-least-once behavior instead of fire-and-forget processing.
- Added a manager-owned active execution registry per instance so destroy-instance flows can drain/terminate all bound executions through a single authoritative mapping, while the scheduler now also skips instances already at concurrency capacity.
- Aligned execution termination semantics inside the manager: when an execution starts it now upserts a running execution response with its bound instance, and terminate/destroy flows update the manager-side execution view immediately before signaling runtime termination.
- Standardized termination naming across manager, host API, and runtime layers: manager methods now use `request_*` for control-plane termination requests, registry helpers use `request/read/clear_*_request`, and runtime WASM hooks use `read_*` / `abort_*` naming to make signal-vs-consumption boundaries explicit.
- Continued the naming cleanup inside `TaskExecutionManager`: helpers now distinguish `sync_*` for SMS/local state projection, `stop_and_unregister_*` for runtime stop plus manager/scheduler/task deregistration, and `persist_*_response` for writing queryable execution response snapshots.
- Promoted clearer public manager verbs while keeping compatibility wrappers: new internal call sites now prefer `request_execution_termination(...)` and `drain_and_destroy_instance(...)`, while legacy `terminate_execution(...)` / `destroy_instance(...)` remain as thin wrappers for compatibility.
- Added explicit instance deletion semantics to the SMS control plane: `InstanceRegistryService` now supports `DeleteInstance`, `TaskExecutionManager` calls it after local unregister, and the SMS instance index stores deletion tombstones so stale async upserts cannot resurrect a removed instance.
- Continued the naming entropy reduction for execution / SMS modules: status adapters now follow `source_to_target`, SMS reporter methods use `report / update / delete / acknowledge`, SMS projector/state-store methods use `upsert_*_record / tombstone_*_record / project_*`, and local SMS materialization uses `materialize_*`, so similarly named methods now operate at the same abstraction layer.
- Further aligned the control-plane model around “task is workload spec, instance binds to node”: `task.node_uuid` and `RegisterTaskRequest.node_uuid` are now removed from the task protocol surface, tasks gain `desired_replicas` and `scheduling_strategy` (with an initial `SPREAD` implementation), task events are delivered on a global task stream to all spearlets, and task deletion now finalizes only after SMS observes that the task’s active instances have drained instead of relying on single-node ownership.
- Refactored `TaskExecutionManager` flow methods into named stages such as resolve, begin, park, and finalize helpers.
- Refactored `handle_async_completion()` into release, record, finalize, and store stages.
- Moved `TaskExecutionManager` tests out of `manager.rs` into `src/spearlet/execution/manager_tests.rs`.
- Refactored instance acquisition/creation flow into selection, capacity check, config preparation, create/start, and registration stages.
- Removed the legacy `TaskEventBus` and `SubscribeTaskEvents` RPC; spearlet task sync now consumes unified events via `EventsService.SubscribeEvents`.
- Centralized public task/instance/execution status strings and common status predicates into `src/sms/types.rs`.
- Updated `TaskExecutionManager` to report SMS instance/task state through the adapter instead of scattering proto enum choices across manager flow code.
- Kept external task APIs stable while reducing the size and mixed responsibilities of `service.rs`, `web_admin.rs`, and `task_events.rs`.

### 5. Integration Tests Update

**File**: `tests/task_integration_tests.rs`

**Changes**:
- Updated test data generation
- Modified test scenarios to use new API
- Fixed priority value mappings
- Updated error handling tests
- All tests now pass successfully

## API Usage Examples

### Register a Task

```bash
curl -X POST http://localhost:8080/api/v1/tasks \
  -H "Content-Type: application/json" \
  -d '{
    "description": "Test task",
    "priority": "normal",
    "endpoint": "http://worker:8080/execute",
    "version": "1.0.0",
    "capabilities": ["compute", "storage"],
    "config": {
      "timeout": 300,
      "retries": 3
    }
  }'
```

### List Tasks

```bash
curl -X GET "http://localhost:8080/api/v1/tasks?status=registered&priority=normal"
```

### Get Task Details

```bash
curl -X GET http://localhost:8080/api/v1/tasks/{task_id}
```

### Delete a Task

```bash
curl -X DELETE http://localhost:8080/api/v1/tasks/{task_id} \
  -H "Content-Type: application/json" \
  -d '{
    "reason": "operator cleanup",
    "force": true
  }'
```

## Priority Levels

The system supports the following priority levels:
- `low` - Low priority tasks
- `normal` - Normal priority tasks (default)
- `high` - High priority tasks
- `urgent` - Urgent priority tasks

## Benefits of Refactoring

1. **Simplified API**: Reduced complexity from lifecycle management to registration model
2. **Better Performance**: Removed unnecessary state transitions
3. **Clearer Semantics**: Registration-based model is more intuitive
4. **Easier Testing**: Simplified test scenarios and better test coverage
5. **Maintainability**: Cleaner code structure and reduced complexity

## Migration Notes

For existing clients using the old API:
- Replace `submit_task` calls with `register_task`
- Replace `stop_task` and `kill_task` calls with `unregister_task`
- Update task data structures to include new fields (endpoint, version, capabilities, config)
- Update priority values to use lowercase strings (normal, high, etc.)

## Testing

All integration tests have been updated and are passing:
- `test_task_lifecycle` - Tests complete task registration and unregistration
- `test_task_list_with_filters` - Tests task listing with various filters
- `test_task_error_handling` - Tests error scenarios
- `test_task_sequential_operations` - Tests multiple task operations
- `test_task_content_types` - Tests different content types

Run tests with:
```bash
cargo test --test task_integration_tests
```
