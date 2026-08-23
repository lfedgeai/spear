# SMS Node And Resource API Cleanup

## Scope

This cleanup targets duplicated node/resource presentation logic in the SMS module:

- `src/sms/handlers/node.rs`
- `src/sms/handlers/resource.rs`
- `src/sms/web_admin.rs`
- `src/sms/node_api.rs`

## Architectural Change

Before this cleanup, node and resource responses were manually assembled in multiple places:

- Public HTTP handlers built ad-hoc `json!` payloads
- Admin node list built its own row projection
- Admin node detail used a different local presenter path

That made the controller layer carry presentation knowledge and increased the chance of field drift between public and admin surfaces.

This cleanup introduces `src/sms/node_api.rs` as a shared mapping layer for:

- Public node responses
- Public node resource responses
- Public node-with-resource responses
- Admin node list items
- Admin node detail envelopes

After the change:

- `handlers/node.rs` is a thinner transport adapter
- `handlers/resource.rs` is a thinner transport adapter
- `web_admin.rs` reuses shared typed node view models instead of rebuilding JSON rows
- Node/resource response shapes now have a single internal home

## Benefits

- Reduces duplicated field projection logic across public and admin APIs
- Makes controller files shorter and easier to scan
- Clarifies where node/resource presentation rules belong
- Establishes a cleaner modular pattern for future API cleanup work

## Verification

- `cargo test node_api --lib`
- `cargo test handlers_test --lib`
- Targeted diagnostics for:
  - `src/sms/handlers/node.rs`
  - `src/sms/handlers/resource.rs`
  - `src/sms/node_api.rs`
  - `src/sms/web_admin.rs`
