# snailbox
Snails in one place

## Usage

```sh
id=$(snailbox create --from alice --to bob --to carol --subject 'lunch?' --body 'Noon?')
snailbox show "$id"          # readable text, creation time in local time
snailbox show "$id" --json   # JSON, creation time in RFC 3339 UTC
snailbox create --from bob --to alice --subject 're: lunch?' --reply-to "${id:0:8}"
snailbox list                # one line per Message (at most 50), oldest stored first
snailbox list --to bob --from alice --json
snailbox list --after "${id:0:8}" --limit 20   # catch up from the last Message handled
snailbox list --thread "${id:0:8}"             # the Thread a Message belongs to, no side branches
```

`create` needs `--from` (there is no default Sender and no fallback to the operating-system
user or Git identity), at least one `--to` and `--subject`; without them, or with a blank
`--from`, `--to` or `--subject`, it exits non-zero with an error on standard error; `list`
refuses a blank `--from` or `--to` the same way, while a name that matches nothing lists
nothing and exits 0.
The body comes from at most one of `--body <text>`, `--body-file <path>` or `--stdin` (its
Body source), so long text needs no shell quoting:

```sh
git log -5 | snailbox create --from alice --to bob --subject 'recent work' --stdin
```

Giving two body sources is an error, and so is an empty `--body ''` (leave the body out
for none), which clap refuses while parsing the arguments, and a `--body-file` or `--stdin`
input that is empty or only whitespace, so an upstream command that failed before printing
anything cannot store a blank Message (one that failed after printing some text still gives
a body). With no Body source the Message has no body. Subject and body have no length cap of
snailbox's own; the operating system limits the total size of the command-line arguments, which caps
`--subject` and `--body` but not `--body-file` or `--stdin`.

`create` prints only the new Message's full id, a random UUID version 4. `--reply-to` stores
the Message as a Reply to the named Message, and `show` prints a Reply's parent id (a
`Parent:` line in text, `parent` in JSON, null when the Message is not a Reply). `show` and
`--reply-to` take a Message's id or any non-empty prefix of it, in either letter case, that
matches exactly one Message; they exit non-zero with an error on standard error when the
prefix is empty or no Message matches, or, listing the matches, when several do. They never
take a date or time: ids are random UUIDs, so a date or time matches no Message, and the
no-match error adds that the argument or flag takes a Message id, not a date or time, and to
run `snailbox list` to find one. The exception is an all-digit value such as `20261126`, which
is also a valid id prefix: it is read as one, and matches a Message only when that Message's
id happens to start with it. There is no daemon: `create`, `show` and `list` each open the
Inbox directly.

`list` always returns Messages in sequence-number order, oldest first: the order this Inbox
stored them, which creation times never decide (ADR 0001). There is no sort flag. Each Message's
text line is tab-separated: the first eight characters of the id, the creation time in local
time, the Sender, the Receivers joined by commas and the subject; a tab or line break inside
one of them prints as a space. A limit that left Messages out adds one last line with no tabs
(below). `--json` always prints one object, `{"messages": [...], "left_out": <count>,
"next": <flag and id, or null>}` (below). Each element of `messages` holds what the text line
does, with the full id: `id`, `created_at` (RFC 3339 UTC), `from`, `to` and `subject`. It has
no `body` or `parent`; `show --json` has both. `--to <name>` keeps Messages with that name
among their Receivers, `--from <name>` keeps Messages from that Sender, and both together
keep only Messages satisfying both. An empty Inbox, or filters matching nothing, exit 0 and
print nothing in text form, or with `--json` an object whose `messages` is empty.

`--after <id>` and `--before <id>` are cursors: they keep the Messages this Inbox stored after
or before the named one, and together bound a range. They take a Message id or a unique
prefix, never a date or time, with the same errors as `show`: several-match, and no-match
with its hint that the flag takes a Message id, its next step and its all-digit exception.

`--upthread <id>`, `--downthread <id>` and `--thread <id>` follow Replies from the named
Message, which they always include: `--upthread` adds the chain of Messages it replies to, back
to one that is not a Reply; `--downthread` adds every Reply beneath it at any depth; `--thread`
adds both. Replies on other branches are left out of all three. They take a Message id or a
unique prefix, with the same errors as `--after` (several-match, and no-match with its hint
that the flag takes a Message id, its next step and its all-digit exception), keep
sequence-number order, and combine with every other filter; when given together they all
apply. So `--downthread <id> --after <last-seen-id>` returns only the new Replies beneath a
Message.

`--limit <n>` defaults to 50 and must be at least 1; `--limit 0` is refused. When more Messages
match than the limit, `--after` (alone or with `--before`) returns the first n after that
Message, `--before` alone returns the last n before it, and with neither `--after` nor
`--before`, the most recent n; either way they print oldest first. A limit that left Messages
out ends the text output with a line giving how many and the flag (`--after <id>` or
`--before <id>`, with a full id) that reads them; the flag replaces the same cursor in the
command (any `--before` for the older ones, any `--after` for the newer ones), whose other flags
stay. With `--json`, `left_out` is that count and `next` that flag and id; when nothing was left
out `left_out` is 0 and `next` is null, so the object has one shape either way. The cursors and
limit combine with `--to` and `--from`, which narrow the matches before the limit applies, as do
the thread filters.

The Inbox is a SQLite database in the folder `SNAILBOX_HOME` names when it is set, else
`$XDG_DATA_HOME/snailbox/` when that is set, else `~/.local/share/snailbox/`. An empty
variable counts as unset. `create`, `show` and `list` each create a missing folder, readable
and writable by its owner only; a folder that already exists keeps its permissions. The
database file `inbox.sqlite3`, when snailbox creates it, is readable and writable by its owner
only whatever the folder's permissions, so other users cannot read the Messages even in a
folder they can list; a database file that already exists keeps its permissions.

Several processes can run `create` against one Inbox at once: SQLite lets one write at a
time, and the others wait for it, up to 30 seconds each, before failing with "database is
locked". Every Message they store gets its own place in sequence-number order, so paging with
`--after` sees each one exactly once.

## Development

`cargo build` builds the `snailbox` binary; `snailbox --help` lists its commands.
`make check` runs the formatter check, the linter (warnings fail it), the tests and the
Markdown checks. The GitHub workflow `.github/workflows/check.yml` runs the same command on
every pull request and every push to `main`. It uses the latest stable Rust toolchain while
`RUSTUP_TOOLCHAIN` in that file is `stable`; set it to a version to pin the toolchain.
