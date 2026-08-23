# Spearlet Function Service Cleanup

## Scope

This cleanup targets execution and invocation flow readability in:

- `src/spearlet/function_service.rs`

## Architectural Change

Before this cleanup, `function_service.rs` mixed several concerns directly inside gRPC handlers:

- invoke request normalization
- execution-to-proto mapping
- payload shaping
- execution error mapping
- terminate error translation

That made the handlers longer and harder to scan because transport conversion details were interleaved with the actual business action.

This cleanup introduces a thinner internal structure:

- `normalize_invoke_request()` centralizes defaulting and request normalization
- `payload_with_content_type()` centralizes output payload shaping
- `execution_error_to_proto()` centralizes execution error projection
- `invoke_response_from_execution()` builds invoke responses from execution results
- `execution_to_proto()` builds execution responses consistently
- `map_terminate_execution_error()` centralizes terminate error translation

After the change:

- `invoke`, `get_execution`, `list_executions`, and `terminate_execution` focus more on control flow
- transport mapping rules live in small dedicated helpers
- execution response behavior is more consistent across endpoints

## Benefits

- Improves readability of the command path in `function_service.rs`
- Reduces repeated execution/proto mapping code
- Makes future execution endpoints easier to add without re-encoding the same response rules
- Keeps the refactor lightweight and local instead of introducing unnecessary abstraction layers

## Verification

- `cargo test function_service --lib`
- `cargo test grpc_server --lib`
- `cargo test http_gateway --lib`

Added focused tests for:

- invoke request default normalization
- hiding execution output when output is not requested
