---
status: accepted
---

# Random Message ids, with a sequence number kept per inbox

A Message's id is a random UUID (version 4) that snailbox generates when the Message is
created, so the id stays unique when messages move between machines. Each inbox separately
gives each message a sequence number when it stores it, and `list` returns messages in
sequence-number order; `list --after <id>` and `list --before <id>` return what this inbox
stored after or before that Message. The creation
timestamp is displayed and never used for ordering. Commands accept any id prefix that
matches exactly one Message in the inbox.

## Considered options

- **A counter assigned by the store.** Short to type and a safe cursor, since a higher number
  always means stored later. Rejected because a number is unique only within one inbox, so
  ids would collide when an inbox is migrated or merged.
- **A time-ordered id (UUID version 7 or ULID) used as the cursor too.** Rejected on two
  counts. When two `create` commands run at once, each stamps its id before writing, so the
  earlier id can be stored second and a consumer already past it would never see that
  Message. And the leading characters are the timestamp, so messages created within about a
  minute of each other share their first eight characters and a short prefix does not
  identify one.

## Consequences

- An inbox's sequence numbers belong to that inbox. After a migration or merge the receiving
  inbox assigns its own, so a Message created last week can be stored today and still
  appears after a consumer's `--after` position.
- An id says nothing about when its Message was created; readers use the timestamp for that.
