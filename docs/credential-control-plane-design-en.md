# Credential Control Plane Design

This document describes how to introduce a runtime-manageable, syncable, and hot-reloadable credential control plane for AI backend `credential_ref` within the current `spear` architecture.

Goals:

- Keep only `credential_ref` inside `BackendSpec`, never plaintext secrets
- Reuse the existing SMS `snapshot + revision + watch` architecture
- Reuse the spearlet-side `list + watch + periodic refresh` sync pattern
- Support runtime add / update / delete for credentials
- Let adapters read the latest credential at request time without restarting spearlet
- Introduce as few new abstractions as possible and avoid unnecessary complexity

---

## 1. Current problem

The old `credential_ref` behavior was effectively:

```text
BackendSpec.credential_ref
-> SpearletConfig.ai.credentials[].name
-> api_key_env
-> std::env::var(api_key_env)
```

Problems with that approach:

- It depends on process environment variables and is unfriendly to runtime updates
- In Docker Compose and container environments, dynamically adding keys requires restart or awkward workarounds
- Credential lifecycle is coupled to backend lifecycle
- Web Admin can only fill a ref, but cannot manage the credential itself
- Backend construction resolved secrets statically, which is poor for key rotation

So `credential_ref` needs to evolve from a startup-time config lookup into a runtime-resolved reference.

---

## 2. Design principles

### 2.1 Keep `credential_ref`

`BackendSpec` should continue to store only:

- `credential_ref: String`

It should not directly store:

- Plaintext API keys
- Environment variable names
- File paths

This keeps backend spec as a resource reference instead of a secret carrier.

### 2.2 Reuse the existing control-plane pattern

Prefer reusing the existing patterns:

- SMS:
  - `AdminBackendsState`
  - `RegistryWatchHub`
  - `KvStore`
- spearlet:
  - `BackendAssignmentController`
  - `DynamicBackendRegistry`

The credential control plane should not introduce a completely different synchronization model.

### 2.3 Resolve credentials at call time

Do not resolve the key into a static value during adapter construction.

Instead:

- The adapter keeps `credential_ref`
- The adapter reads the current value from local credential store right before sending a request or opening a websocket

That makes runtime key updates take effect naturally.

### 2.4 Build a minimal usable Phase 1 first

Phase 1 focuses only on:

- Encrypted credential storage inside SMS
- Web Admin CRUD
- spearlet watch-based synchronization
- Runtime resolution by ref inside adapters

Phase 1 intentionally excludes:

- Vault / KMS / external secret providers
- Complex RBAC
- Node placement
- Per-node credential capability matrix
- Automatic rotation policies

---

## 3. Overall architecture

```text
Web Admin
  -> AdminCredentialService (SMS)
       -> AdminCredentialsState
            -> KV snapshot
            -> revision
            -> watch hub

Spearlet
  -> CredentialSyncService
       -> list credentials
       -> watch credentials
       -> periodic refresh
       -> DynamicCredentialStore

AI Adapter
  -> credential_ref
  -> CredentialProvider
  -> resolve current key at request/connect time
```

---

## 4. SMS-side design

### 4.1 New state module: `AdminCredentialsState`

Recommended file:

- `src/sms/admin_credentials.rs`

The structure can reference the current unified control-plane persistence modules, such as `src/sms/ai_backends/repository_kv.rs`.

Core structure:

```rust
pub struct AdminCredentialsState {
    kv: Arc<dyn KvStore>,
    snapshot: RwLock<CredentialSnapshot>,
    watch: RegistryWatchHub<CredentialEvent>,
    cipher: Option<Arc<dyn SecretCipher>>,
}
```

Notes:

- SMS is allowed to start without `SMS_CREDENTIAL_MASTER_KEY`
- Credential metadata listing can still work when the master key is missing
- Operations that need secret material, such as `upsert` and `list_materials`, must return a clear `FailedPrecondition`
- This avoids blocking unrelated SMS functionality while still enforcing explicit key configuration for secret management

### 4.2 Storage model

```rust
struct CredentialRecord {
    name: String,
    provider_kind: String,
    encrypted_secret: Vec<u8>,
    version: u64,
    description: String,
    disabled: bool,
    created_at_ms: i64,
    updated_at_ms: i64,
}
```

```rust
struct CredentialSnapshot {
    revision: u64,
    credentials: Vec<CredentialRecord>,
}
```

Notes:

- `name` is the unique control-plane reference
- In Phase 1, `provider_kind` is effectively `inline_encrypted`
- `encrypted_secret` stores only encrypted material, never plaintext
- `version` tracks the version of a single credential
- `revision` tracks the sync version of the whole credential registry

### 4.3 Encryption abstraction

Phase 1 uses a small abstraction:

```rust
pub trait SecretCipher: Send + Sync {
    fn encrypt(&self, plaintext: &str) -> Result<Vec<u8>, SmsError>;
    fn decrypt(&self, ciphertext: &[u8]) -> Result<String, SmsError>;
}
```

Recommended implementation:

- `AesGcmSecretCipher`

Key source:

- A master key from the SMS process environment

Current runtime semantics:

- SMS may boot without the environment variable
- Secret-management operations require the variable to be configured before use

Benefits:

- Much safer than storing plaintext inside KV
- No need to introduce an external KMS immediately
- Easy to upgrade later to a KMS-backed cipher

### 4.4 CRUD behavior

Required operations:

- `list()`
- `upsert(name, secret, description, disabled)`
- `delete(name)`
- `watch_credentials(since_revision)`

Like `AdminBackendsState`:

- Revision increases after mutations
- The updated snapshot is persisted back into KV
- Incremental events are pushed through `RegistryWatchHub`

### 4.5 Delete and reference checks

Phase 1 recommendation:

- Reject deletion if the credential is still referenced by a backend

Reasons:

- Simpler behavior
- Avoids dangling `credential_ref`
- Easy to understand

SMS can validate references directly against unified `ai_backends` records.

---

## 5. proto / gRPC API design

Use a dedicated service instead of folding this into a legacy configuration service.

Recommended service:

- `AdminCredentialService`

Recommended proto file:

- `proto/sms/admin_credentials.proto`

### 5.1 Message definitions

The API should expose:

- `CredentialInfo`
- `CredentialMaterial`
- `CredentialEvent`

Important behavior:

- `CredentialInfo` must not return secrets
- `CredentialMaterial` is for spearlet synchronization, not for general Web Admin listing
- `CredentialEvent` carries registry revision plus upsert / delete names for incremental sync

### 5.2 Admin management API

Required RPCs:

- `ListCredentials`
- `UpsertCredential`
- `DeleteCredential`
- `WatchCredentials`

These are the control-plane CRUD APIs used by Web Admin.

### 5.3 Spearlet sync API

Required RPCs:

- `ListCredentialMaterials`
- `WatchCredentialMaterials`

These are runtime synchronization APIs for spearlet. They may fail with `FailedPrecondition` until `SMS_CREDENTIAL_MASTER_KEY` is configured.

---

## 6. Web Admin design

### 6.1 Dedicated UI surface

Recommended UX:

- A dedicated credentials section in the AI backend admin area

Current implementation status:

- A dedicated Web Admin route now exists at `/ai-backends/credentials`
- The Remote AI Models page now links to that shared credentials page instead of embedding only a minimal inline form

### 6.2 Page capabilities

The page should support:

- Listing credentials
- Showing name, description, disabled state, version, and last update time
- Creating credentials
- Editing credentials
- Deleting credentials

Current implementation status:

- Listing, search, status filtering, and updated-time display are implemented
- Create dialog is implemented
- Edit dialog is implemented
- Leaving the secret blank during edit now keeps the current encrypted secret and updates metadata only
- Delete confirmation is implemented

### 6.3 Security behavior

Web Admin should:

- Never render plaintext secrets in credential list responses
- Avoid logging secrets in frontend requests or backend logs
- Treat credential creation and updates as write-only flows

### 6.4 Backend form integration

Remote backend forms should:

- Use a credential selector instead of a freeform `credential_ref` text box
- Load available credentials from the backend
- Keep the backend spec referencing only the selected credential name

---

## 7. spearlet-side design

### 7.1 Add `DynamicCredentialStore`

Recommended responsibilities:

- Hold the latest credential snapshot in memory
- Index by credential name
- Support full refresh and incremental updates
- Expose read APIs for runtime adapters

### 7.2 Add `CredentialSyncService`

Recommended flow:

- Initial list on startup
- Watch for incremental changes
- Periodic refresh for self-healing
- Update `DynamicCredentialStore`

### 7.3 Why reuse this pattern

Reasons:

- It already matches the remote-backend sync model
- It is easy to reason about
- It gives natural recovery after temporary watch disconnects

---

## 8. Runtime adapter resolution

### 8.1 Current problem

The old model bound backend availability to startup-time secret resolution.

### 8.2 Recommended change

Adapters should no longer store only a resolved static key. They should instead keep:

- Optional static API key for explicit test / direct config scenarios
- Optional `credential_ref`
- Optional credential resolver

### 8.3 What the adapter stores

At runtime the adapter should retain only:

- Backend identity and URL data
- `credential_ref`
- A resolver/provider handle

### 8.4 Resolve during the call

Before each outbound request or websocket connect:

- Check explicit static key first if present
- Otherwise resolve the current secret from the credential store
- Return clear errors such as `credential_missing`, `credential_disabled`, or `credential_not_synced`

### 8.5 Recommended behavior

- Do not drop the backend merely because the credential is temporarily unavailable
- Keep backend registration stable
- Fail only at invocation time when the credential is actually required

Current implementation status:

- Adapters now distinguish `credential_missing`, `credential_disabled`, and `credential_not_synced`
- `backend_reporter` now reuses the same credential resolution logic so node-reported backend status stays aligned with runtime behavior

---

## 9. Relationship with existing backend sync

Credential sync should stay parallel to backend sync, not embedded inside backend payloads.

Reasons:

- Better separation of concerns
- Cleaner evolution of the control plane
- Avoids rebroadcasting secrets through backend registry snapshots

---

## 10. State and observability

### 10.1 spearlet local state

It should be possible to observe:

- Current credential revision
- Number of locally cached credentials
- Last successful sync time
- Last sync error
- Watch connectivity

Current implementation status:

- Spearlet now exposes a local read-only endpoint at `/monitoring/ai/credentials`
- The endpoint currently returns `status`, `started`, `watch_connected`, `applied_revision`, `local_store_epoch`, `credential_count`, `last_success_at_ms`, and `last_error`
- SMS Web Admin can proxy per-node status through `/admin/api/nodes/{uuid}/ai/credentials`

### 10.2 Backend invocation errors

When a backend invocation fails because of credential issues, the error should be explicit and actionable, for example:

- Missing credential ref
- Credential not yet synced
- Credential disabled

### 10.3 Web Admin page

The admin UI should show enough metadata to understand:

- Which credential exists
- Whether it is referenced
- When it was last updated
- Whether it is disabled

Current implementation status:

- The Credentials page now shows `referenced_by_count`
- The Node Detail page now includes a `Credential Sync` card for per-node local sync visibility

Phase 1 does not include:

- Node-level credential presence matrix
- Node sync lag dashboard

---

## 11. Security boundaries

### Must have

- SMS KV never stores plaintext secrets
- Web Admin list/API never returns plaintext in normal listing flows
- Logs never print secrets
- Debug/status endpoints never return secrets
- spearlet stores secrets in memory only

### Acceptable in Phase 1

- Master key comes from an SMS environment variable
- In-memory local secret storage

### Future extensions

- KMS-backed encryption
- Audit logs
- RBAC
- Secret rotation history

---

## 12. Phased implementation

### Phase 1: minimal control-plane loop

- proto: credential CRUD + watch
- SMS: `AdminCredentialsState`
- SMS: encrypted KV storage
- spearlet: `DynamicCredentialStore`
- spearlet: `CredentialSyncService`
- adapter: runtime resolve by `credential_ref`

Goal:

- Fully connect the runtime add/update/delete key path

### Phase 2: Web Admin productization

- Credential list page
- Create/edit/delete dialogs
- Backend form uses credential selector

Current implementation status:

- Dedicated credentials page is implemented
- Create/edit/delete dialogs are implemented
- Remote backend forms already use a credential selector

### Phase 3: observability and security hardening

- Reference visibility
- Audit logs
- Per-node sync state
- KMS / external secret provider

Current implementation status:

- A lightweight reference visibility view is implemented via `referenced_by_count`
- A minimal per-node sync status view is implemented in the Node Detail page
- Audit logs and KMS / external secret provider are still not implemented

---

## 13. Approaches not recommended

### 13.1 Continue relying on hot-updated env vars

Problems:

- Container environment variables are not a good fit for runtime updates
- Compose / systemd workflows become awkward

### 13.2 Store plaintext keys directly in backend spec

Problems:

- High risk of control-plane leakage
- Poor auditability and weak least-privilege boundaries

### 13.3 Freeze keys during adapter construction

Problems:

- Runtime key add/update becomes unnatural
- Rotation requires adapter rebuild or process restart

---

## 14. Recommended implementation order

Recommended sequence:

1. Add credential proto and SMS state
2. Add spearlet dynamic credential store and sync
3. Switch adapters to runtime resolution
4. Build the Web Admin credentials UI
5. Convert backend forms from freeform `credential_ref` to a selector

Why this order works:

- Backend infrastructure lands first
- UI comes after the runtime path is ready
- It avoids exposing a UI before the runtime behavior is actually supported

---

## 15. One-sentence summary

The most natural, elegant, and extensible solution for the current architecture is to model credentials as a control-plane resource parallel to remote backends, managed by SMS through `snapshot + revision + watch`, synchronized by spearlet through `list + watch + refresh`, and resolved dynamically by adapters at request time via `credential_ref`.
