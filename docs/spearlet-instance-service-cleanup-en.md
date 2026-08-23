# Spearlet Instance Service Cleanup

## Scope

This cleanup targets instance-level command flow in:

- `src/spearlet/instance_service.rs`

## Architectural Change

Before this cleanup, `instance_service.rs` was already small, but its handlers still directly mixed:

- execution manager access
- optional-string normalization
- internal error translation
- post-destroy task reconciliation

That made the file feel more like a thin pass-through than a clearly shaped instance-level orchestration boundary.

This cleanup introduces a clearer internal structure:

- `execution_manager()` centralizes access to the execution manager dependency
- `optional_non_empty()` and `task_filter()` normalize request-level optional strings
- `map_internal_error()` centralizes internal error conversion
- `reconcile_task_after_destroy()` captures the post-destroy compensation step explicitly

After the change:

- `destroy_instance()` reads more like an instance-level command workflow
- `reconcile_task_assignments_now()` reads more like a dedicated orchestration command
- the file better communicates its role as the instance service boundary

## Benefits

- Improves readability without over-engineering a very small service
- Makes instance-level orchestration steps easier to recognize
- Keeps normalization and error handling rules consistent
- Aligns the service with the same “thin endpoint + focused helper” direction used in other spearlet modules

## Verification

- `cargo test instance_service --lib`
- `cargo test grpc_server --lib`
- `cargo test http_gateway --lib`

Added focused tests for:

- optional string normalization
- task filter normalization
