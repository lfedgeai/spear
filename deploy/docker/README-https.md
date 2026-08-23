# Docker Compose HTTPS Overlay

This setup follows the "base compose + HTTPS overlay" pattern:

- `compose.local.yaml`: core services only
- `compose.https.yaml`: external TLS termination with Caddy

## Exposed HTTPS endpoints

- `https://<host-or-ip>:18443/admin` -> SMS Web Admin
- `https://<host-or-ip>:18444/console` -> SMS Console
- `https://<host-or-ip>:18445/` -> Debug Server

`<host-or-ip>` can be an IP address when no domain is available yet.

## Notes

- Application containers still serve plain HTTP internally.
- Caddy terminates TLS externally with `tls internal`.
- This overlay only exposes HTTPS for:
  - SMS Web Admin
  - SMS Console
  - Debug Server
- Existing direct HTTP ports remain available for debugging unless you remove them from the base compose file:
  - `18080` -> SMS HTTP
  - `18082` -> SMS Web Admin
  - `18081` -> Spearlet HTTP

## Start without HTTPS

```bash
docker compose -f deploy/docker/compose.local.yaml up -d --build
```

## Start with HTTPS overlay

```bash
docker compose \
  -f deploy/docker/compose.local.yaml \
  -f deploy/docker/compose.https.yaml \
  up -d --build
```

## Host / IP certificate subject

By default, Caddy issues a local certificate for `127.0.0.1`.

If you want to access from another IP, set `SPEAR_HTTPS_HOST` to the host IP before startup:

```bash
export SPEAR_HTTPS_HOST=192.168.1.10
docker compose \
  -f deploy/docker/compose.local.yaml \
  -f deploy/docker/compose.https.yaml \
  up -d --build
```

Then access:

- `https://192.168.1.10:18443/admin`
- `https://192.168.1.10:18444/console`
- `https://192.168.1.10:18445/`

## Optional port overrides

- `SMS_WEB_ADMIN_HTTPS_PORT`
- `SMS_CONSOLE_HTTPS_PORT`
- `DEBUG_SERVER_HTTPS_PORT`

If these environment variables are set, the actual host ports may differ from the defaults above. Check the live mapping with:

```bash
docker compose \
  -f deploy/docker/compose.local.yaml \
  -f deploy/docker/compose.https.yaml \
  ps https-proxy
```

## Debug server

The local debug server is also mapped to the host for easier log inspection:

- `http://127.0.0.1:17777/` - lightweight log viewer with session/client/stream dropdown selectors and console-aligned dark styling
- `http://127.0.0.1:17777/health`
- `http://127.0.0.1:17777/logs`
- `http://127.0.0.1:17777/clients`
- `http://127.0.0.1:17777/streams`

If you prefer to access it through the HTTPS overlay, use:

- `https://127.0.0.1:18445/`

The debug server is now built from the Rust `debug-server` binary and packaged into the local Docker image.

Logs are now organized as:

- `session`
  - `client`
    - `stream`

Default values:

- `default-session`
- `default-client`
- `default-stream`

Routing rules:

- if an event contains `clientName`, that client is used
- if an event contains `streamName`, that stream is used
- missing values fall back to the default names above

In the local compose setup, `spearlet` sends:

- `DEBUG_SESSION_ID=${DEBUG_SESSION_ID:-default-session}`
- `DEBUG_CLIENT_NAME=${DEBUG_CLIENT_NAME:-default-client}`
- `DEBUG_STREAM_NAME=${DEBUG_STREAM_NAME:-default-stream}`

When the `debug-reporter` path is enabled, mirrored WASM guest logs are sent to:

- `streamName=wasm-log`
- `data.taskId`
- `data.executionId`
- `data.instanceId`

See also:

- `../../docs/debug-server-log-mirroring-en.md`
- `../../docs/debug-server-log-mirroring-zh.md`

Optional override:

- `DEBUG_SERVER_PORT`
- `DEBUG_SESSION_ID`
- `DEBUG_CLIENT_NAME`
- `DEBUG_STREAM_NAME`

## Troubleshooting

### Console HTTPS needs API passthrough

The console page is served from `/console`, but the frontend also calls same-origin APIs such as `/api/v1/tasks`.

That means the HTTPS console proxy must forward both:

- `/console*`
- `/api/*`

In the current setup, the `18444` HTTPS entry proxies all non-root requests to `sms:8080`, while `/` is redirected to `/console`.

### Reloading Caddy changes

After updating `deploy/docker/caddy/Caddyfile`, force-recreate the HTTPS proxy to ensure the new config is loaded:

```bash
docker compose \
  -f deploy/docker/compose.local.yaml \
  -f deploy/docker/compose.https.yaml \
  up -d --force-recreate https-proxy
```
