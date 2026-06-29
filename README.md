# smails-rs

Rust spike for checking whether the smails Cloudflare shape can be implemented
with workers-rs:

- Worker `fetch`
- Durable Object binding
- Durable Object SQLite storage
- Durable Object alarms
- hibernatable WebSocket callbacks
- Rust-exported `email` handler

This is intentionally not the product rewrite yet. It keeps a tiny static
asset shell and caller-provided test mailbox data so the platform pieces stay
visible.

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
    src/lib.rs             # wasm frontend helpers shared with browser code
    static/index.html      # minimal Workers Assets shell
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
