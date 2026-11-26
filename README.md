# snailbox
Snails in one place

## Usage

```sh
id=$(snailbox create --from alice --to bob --to carol --subject 'lunch?' --body 'Noon?')
snailbox show "$id"          # readable text, creation time in local time
snailbox show "$id" --json   # JSON, creation time in RFC 3339 UTC
snailbox create --from bob --to alice --subject 're: lunch?' --reply-to "${id:0:8}"
snailbox list                # one line per Message, oldest stored first
snailbox list --to bob --from alice --json
```

`create` needs `--from` (there is no default Sender and no fallback to the operating-system
user or Git identity), at least one `--to` and `--subject`; without them, or with a blank
`--from` or `--to`, it exits non-zero with an error on standard error. The body comes from at
most one of `--body <text>`, `--body-file <path>` or `--stdin` (its Body source), so long text
needs no shell quoting:

```sh
git log -5 | snailbox create --from alice --to bob --subject 'recent work' --stdin
```

Giving two body sources is an error, and so is a `--body-file` or `--stdin` input that is
empty or only whitespace, so an upstream command that failed before printing anything cannot
store a blank Message (one that failed after printing some text still gives a body). With no
Body source the Message has no body. Subject and body have no length cap of snailbox's own;
the operating system limits the total size of the command-line arguments, which caps
`--subject` and `--body` but not `--body-file` or `--stdin`.

`create` prints only the new Message's full id, a random UUID version 4. `--reply-to` stores
the Message as a Reply to the named Message, and `show` prints a Reply's parent id (a
`Parent:` line in text, `parent` in JSON, null when the Message is not a Reply). `show` and
`--reply-to` take a Message's id or any non-empty prefix of it, in either letter case, that
matches exactly one Message; they exit non-zero with an error on standard error when the
prefix is empty or no Message matches, or, listing the matches, when several do. There is no
daemon: `create`, `show` and `list` each open the Inbox directly.

`list` always returns Messages in arrival order, oldest first: the order this Inbox stored
them, which creation times never decide (ADR 0001). There is no sort flag. Each text line is
tab-separated: the first eight characters of the id, the creation time in local time, the
Sender, the Receivers joined by commas and the subject; a tab or line break inside one of
them prints as a space. `--json` prints an array of the
objects `show --json` prints, with full ids. `--to <name>` keeps Messages with that name among
their Receivers, `--from <name>` keeps Messages from that Sender, and both together keep only
Messages satisfying both. An empty Inbox, or filters matching nothing, print nothing (an empty
array with `--json`) and exit 0.

The Inbox is a SQLite database in the folder `SNAILBOX_HOME` names when it is set, else
`$XDG_DATA_HOME/snailbox/` when that is set, else `~/.local/share/snailbox/`. An empty
variable counts as unset. `create` or `show` creates a missing folder, readable and
writable by its owner only; a folder that already exists keeps its permissions.

## Development

`cargo build` builds the `snailbox` binary; `snailbox --help` lists its commands.
`make check` runs the formatter check, the linter (warnings fail it), the tests and the
Markdown checks.
