# smails-rs

Rust implementation of the smails Cloudflare shape with workers-rs:

- Worker `fetch`
- Durable Object binding
- Durable Object SQLite storage
- Durable Object alarms
- hibernatable WebSocket callbacks
- Rust-exported `email` handler

The repo keeps the Worker, Yew frontend, CLI, and MCP server in one Rust
workspace so the Cloudflare platform path stays easy to validate and reuse.

## Layout

```text
Cargo.toml                 # workspace
wrangler.toml              # Cloudflare Worker deploy/dev entrypoint
crates/
  core/
    src/lib.rs             # wasm-safe shared DTOs, paths, token helpers
  native/
    src/lib.rs             # native-only config and HTTP API client
  worker/
    Cargo.toml
    src/lib.rs             # fetch/email/Durable Object entrypoint
  frontend/
    src/                  # Yew CSR app, API client, storage, WebSocket, views
    static/index.html      # Workers Assets shell loading the Rust WASM bundle
  cli/
    src/main.rs            # smails CLI entrypoint
  mcp/
    src/lib.rs             # MCP stdio server
```

## Run

```bash
mise run setup
mise run check
mise run build
mise run dev
mise run cli -- --help
mise run mcp
```

## Configuration

Worker mailbox domains are injected through the `DOMAINS` Worker variable:

```toml
[vars]
DOMAINS = "example.com,alt.example.com"
```

The first domain is used when a create request does not specify one. The
Cloudflare `routes` block in `wrangler.toml` is still deployment config, not a
runtime Worker env var; change it per environment or omit it when deploying to
workers.dev.

Native tools default to `https://smails.dev`, but can target any deployment:

```bash
SMAILS_API_URL=https://mail.example.com smails create
```

Native-only code lives in `crates/native`, `crates/cli`, and `crates/mcp`.
Only `core`, `worker`, and `frontend` are checked for `wasm32-unknown-unknown`
compatibility.

## Probe

```bash
curl -X POST http://127.0.0.1:8787/api/mailbox \
  -H 'content-type: application/json' \
  -d '{"address":"probe","token":"probe.0123456789abcdef0123456789abcdef"}'

curl http://127.0.0.1:8787/api/mailbox/messages \
  -H 'authorization: Bearer probe.0123456789abcdef0123456789abcdef'
```

The local probe covers the production HTTP/auth path. Real Cloudflare Email
Routing must still be checked after deploy, because local HTTP probes do not
prove SMTP ingress.
