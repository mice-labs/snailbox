# snailbox

A local inbox through which agents and people send each other messages asynchronously.
Connectors are a separate tool built on top of it that brings messages in from outside
services and sends replies back out.

## Language

### Inbox

**Inbox**:
The single store of Messages belonging to one user on one machine, shared by every agent and
person sending or receiving there.
_Avoid_: mailbox, queue, database

**Message**:
A unit of content in the Inbox, with a subject and an optional body, addressed from one
Sender to one or more Receivers.
_Avoid_: event, item, mail, notification

**Sender**:
The name a Message is from. It is declared by whoever creates the Message and is neither
registered nor verified.
_Avoid_: author, user, account

**Receiver**:
A name a Message is addressed to. Any name can be a Receiver without being registered first.
_Avoid_: recipient, subscriber, consumer

**Sequence number**:
The integer this Inbox gives each Message when it stores it. It is per Inbox and never reused,
and it is never a date or time. `list` always returns Messages in sequence-number order,
oldest first, and the order never depends on their creation times.
_Avoid_: arrival order, creation order, chronological order, sort order

**Cursor**:
A Message id, or a unique prefix of one, that `list --after` or `--before` takes as a position
in sequence-number order. It is never a date or time.
_Avoid_: offset, timestamp, page token

**Body source**:
Where a Message's body is taken from when it is created: the text given directly, a file, or
standard input. A Message has at most one, and none means a Message with no body.
_Avoid_: body input

**Limit**:
The most Messages `list` returns, set by `--limit`. It is 50 when not given and at least 1; 0 is refused, so there is no unlimited listing.
_Avoid_: page size, count

**Reply**:
A Message that answers one other Message, its **parent**.
_Avoid_: response, follow-up

**Upthread**:
Of a Message: the chain of Messages it replies to, namely its parent, that Message's parent,
and so on back to a Message that is not a Reply.
_Avoid_: ancestors, above

**Downthread**:
Of a Message: every Reply beneath it, namely the Replies to it, the Replies to those, and so
on. Replies on other branches of the same Thread are not downthread of it.
_Avoid_: descendants, below

**Thread**:
Of a Message: that Message together with everything upthread and downthread of it. Replies
on other branches are not part of it, so only the Thread of a Message that is not a Reply
holds every Message that descends from it.
_Avoid_: conversation, chain

### Connectors

**Connector**:
A named connection between snailbox and one account on a service, which an operator adds
and logs in to, such as `work-slack`. Two workspaces on the same service are two Connectors.
_Avoid_: source, plugin instance, integration

**Connector type**:
A separately installed program that implements one way of connecting to one service, such
as Slack over Socket Mode or Slack by polling. Every Connector is of one Connector type, and
one service can have several Connector types.
_Avoid_: plugin, adapter, driver

**Manifest**:
The declaration a Connector type ships with, stating what it needs from snailbox, including
the credentials an operator must supply for each Connector.

**Secret reference**:
A pointer in a Connector's configuration to where a credential's value is kept, given in
place of the value itself.
_Avoid_: secret, credential (for the pointer rather than the value)
