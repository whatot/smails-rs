# API

All mailbox message endpoints require:

```text
Authorization: Bearer <token>
```

## Endpoints

```text
POST /api/mailbox
GET  /api/domains
GET  /api/mailbox/messages
GET  /api/mailbox/messages/:id
DEL  /api/mailbox/messages/:id
WS   /api/mailbox/connect?token=<token>
GET  /health
```

## Native Clients

The CLI and MCP server are native clients. They default to local Wrangler dev
and should be pointed at production explicitly:

```bash
smails create --api-url https://mail.example.com
```

The selected API base URL is saved in the local config with the mailbox token.
For first-time MCP-only setup, pass `api_url` to the MCP `create_mailbox` tool
or set `SMAILS_API_URL` in the MCP server environment.
