# Spearlet gRPC Service Cleanup

## Scope

This cleanup targets the spearlet gRPC service registration path:

- `src/spearlet/grpc_server.rs`
- `src/spearlet/object_service.rs`
- `src/spearlet/function_service.rs`
- `src/spearlet/instance_service.rs`
- `src/spearlet/grpc_server_test.rs`

## Architectural Change

Before this cleanup, `GrpcServer` stored service implementations behind `Arc<T>` and the codebase maintained extra tonic trait implementations for:

- `Arc<ObjectServiceImpl>`
- `Arc<FunctionServiceImpl>`
- `Arc<InstanceServiceImpl>`

Those forwarding impls did not add business value. They only existed to support the registration shape chosen by `GrpcServer`.

This cleanup changes the structure so that:

- `GrpcServer` stores cloneable service implementations directly
- tonic servers are registered with concrete service types
- the low-value `Arc<T>` forwarding service impls are removed
- `HealthService` accepts service values directly while still supporting `Arc<T>` inputs through lightweight conversions for compatibility in tests and helpers

## Benefits

- Reduces indirection in the spearlet service registration path
- Makes `GrpcServer` easier to read because it now owns actual services instead of wrapper pointers
- Removes trait boilerplate that was only forwarding calls
- Keeps service sharing cheap through `Clone`, while making ownership and responsibilities clearer

## Verification

- `cargo test grpc_server --lib`
- `cargo test http_gateway --lib`
- Targeted diagnostics for:
  - `src/spearlet/grpc_server.rs`
  - `src/spearlet/function_service.rs`
  - `src/spearlet/instance_service.rs`

`rust-analyzer` still reports a known false-positive diagnostic in `object_service.rs` that does not reproduce in compilation or tests.
