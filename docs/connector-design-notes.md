# Connector design notes

Status: parked on 2026-11-25. The Store is being designed and built first; this file holds
where the connector design stood when it was set aside. Terms are defined in
[`GLOSSARY.md`](../GLOSSARY.md). Nothing here is implemented.

## Settled

- **Connectors are a separate tool from snailbox** (see
  [ADR 0002](adr/0002-snailbox-holds-only-store-logic.md)). They deliver messages into the
  Store the same way an agent or a person does, and the Store knows nothing about outside
  services. The tool's name and command names are undecided; "refresh" and "add" below name
  the operations, not commands.
- **The connectors program is long-running.** Websockets and webhooks need a process that
  stays up; the Store itself does not.
- **A Connector type is a subprocess speaking a versioned protocol over stdio**, not a native
  library and not WASM. Service SDKs exist mainly in Python and JavaScript, a crashing
  Connector type must not take the host down, and each one should receive only its own
  credentials.
- **One Connector type per way of reaching a service.** Slack by polling and Slack over a
  push connection are two Connector types.
- **A Connector is the named, configured thing an operator adds**, such as `work-slack`. Two
  workspaces on one service are two Connectors of the same Connector type.
- **The Manifest declares the credential fields a Connector type needs. Each Connector's
  configuration gives a Secret reference for each field**, saying where the value is kept.
- **snailbox registers no OAuth app.** Where a service needs one, the operator creates their
  own.
- **No automatic credential recovery in the MVP.** When a service rejects a Connector's
  credentials, the Connector stops and is marked as needing attention.
  A manual refresh of that Connector re-reads its Secret references and restarts it. A
  Secret reference that yields a short-lived value therefore needs that refresh run by hand
  at each expiry, so long-lived tokens are the practical choice until recovery is automatic.
- **Replies go out through the Connector's own mailbox.** A Connector type may declare that
  it handles replies; a reply to one of its messages is delivered to that Connector's mailbox
  for it to send. Starting new outside conversations, and service actions that are not
  messages (acknowledging an incident, adding a reaction), stay with the agent's own tools.

## Assumed from the above, not confirmed

- **No browser login in the MVP.** The connectors program runs no OAuth flow and never stores
  or refreshes a token; services are reached with tokens copied from their dashboards. The
  cost is connecting as yourself to Google and Microsoft accounts where app passwords are
  disabled.

## Open

- **Secret providers.** Proposed: every Secret reference names a provider and a
  provider-specific `key`, as in `{ provider = "onepassword", key = "op://work/slack/bot_token" }`.
  File, environment variable and OS keychain would be built in; any other provider would be a
  helper program found by name, taking the key on stdin and returning the value on stdout.
  This would replace a free-form command reference.
- **Whether adding a Connector saves pasted values to the OS keychain** and writes a
  keychain reference into the configuration, so the file never holds a secret.
- **Sandboxing.** Without one, a Connector type is handed only its own credentials but can
  still read anything the operator's user can. Whether the MVP accepts that, with checksum
  pinning of installed Connector types as the only defence, is undecided. An OS sandbox with
  an egress allowlist would stop a Connector type sending its token to another host, but not
  to another account on an allowed host.
- **The rest of the Manifest**: protocol version, whether the Connector type handles replies,
  whether it needs a webhook route, and setup guidance shown when a Connector is added.
- **Webhook ingress.** Proposed: the connectors program owns one HTTP listener and routes
  each request to the right Connector, so Connector types do not each bind a port.
- **Subscriptions.** Proposed: a Subscription is a rule that delivers messages matching a
  filter into a named mailbox, so two pipelines wanting the same alerts are two rules feeding
  two mailboxes.
- **Which Connector types ship first-party**, out of Slack, Teams, Discord, PagerDuty and
  email.
- **An ADR for the Connector type model and its trust model**, to be written once sandboxing
  is decided.

## Prior art consulted

Recalled from memory during the design session and not checked against current documentation.

- **Subprocess protocol**: HashiCorp `go-plugin` (Terraform providers), LSP, MCP, Airbyte
  connectors, Git and Docker credential helpers.
- **Secret references resolved by a daemon**: Datadog Agent `secret_backend_command`, Vector
  secret backends, Telegraf secret stores, systemd `LoadCredential=`, AWS `credential_process`.
- **Refresh**: the Datadog Agent began with startup-only resolution, then added a manual
  refresh command, then a timer, then reload on a rejected API key.
