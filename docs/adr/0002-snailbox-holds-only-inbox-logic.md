---
status: accepted
---

# snailbox holds only inbox logic

snailbox stores Messages and lets them be created, listed and shown, and nothing more, so
that it stays small enough to run with no daemon, no network access and no configuration.
Identity, authentication and Connectors are separate tools built on top of it, using the same
commands an agent or a person uses.

## Consequences

- **A Sender is whatever name the creator declares.** snailbox does not register names or
  check that a Sender is who it claims to be. This is sound only while one operator owns
  everything writing to the Inbox; a setting with mutually distrusting parties needs an
  identity tool in front of snailbox, not a change to it.
- **snailbox holds no credentials and opens no connections.** Bringing Messages in from
  outside services, and sending Replies back out, belongs to the Connectors tool.
- A feature request that needs snailbox to know who someone is, or to talk to another
  service, is a request for one of those other tools.
