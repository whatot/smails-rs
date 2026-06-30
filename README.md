# smails-rs

Rust implementation of the smails Cloudflare shape with Workers, Durable
Objects, Workers Assets, a Yew frontend, a CLI, and an MCP stdio server.

Original TypeScript project: https://github.com/pexni/smails

## Docs

- [Architecture](docs/architecture.md)
- [API and native clients](docs/api.md)
- [Deployment](docs/deployment.md)

## Local Development

```bash
mise run setup
mise run check
mise run build
mise run dev
```

With `wrangler dev` running:

```bash
mise run probe
mise run cli -- create
mise run cli -- inbox
mise run mcp
```

Local HTTP probes do not prove Cloudflare Email Routing. Real email ingress
must be checked after deployment.

## Deploy

```bash
WORKER_DOMAIN=mail.example.com MAILBOX_DOMAINS=example.com mise run deploy
```
