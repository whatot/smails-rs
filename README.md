# smails-rs

Rust spike for checking whether the smails Cloudflare shape can be implemented
with workers-rs:

- Worker `fetch`
- Durable Object binding
- Durable Object SQLite storage
- Durable Object alarms
- hibernatable WebSocket callbacks
- Rust-exported `email` handler

This is intentionally not the product rewrite yet. It keeps address/token
generation as caller-provided test data so the platform pieces stay visible.

## Layout

```text
Cargo.toml                 # workspace
wrangler.toml              # Cloudflare Worker deploy/dev entrypoint
crates/
  worker/
    Cargo.toml
    src/lib.rs             # fetch/email/Durable Object entrypoint
```

## Run

```bash
mise run dev
```

## Probe

```bash
curl -X POST http://127.0.0.1:8787/api/mailbox \
  -H 'content-type: application/json' \
  -d '{"address":"demo","token":"demo.secret"}'

curl -X POST http://127.0.0.1:8787/__test/email \
  -H 'content-type: application/json' \
  -d '{"to":"demo@smails.dev","from":"sender@example.com","subject":"hello","body":"one time code: 123456"}'

curl http://127.0.0.1:8787/api/mailbox/messages \
  -H 'authorization: Bearer demo.secret'
```

Real Cloudflare Email Routing must still be checked after deploy, because local
HTTP probes only prove the same Rust delivery path, not SMTP ingress.
