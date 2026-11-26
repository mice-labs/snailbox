# snailbox
Snails in one place

## Usage

```sh
id=$(snailbox create --from alice --to bob --to carol --subject 'lunch?' --body 'Noon?')
snailbox show "$id"          # readable text, creation time in local time
snailbox show "$id" --json   # JSON, creation time in RFC 3339 UTC
snailbox list                # one line per Message, oldest stored first
snailbox list --to bob --from alice --json
```

`create` prints only the new Message's full id, a random UUID version 4. `show` takes that
full id and exits non-zero with an error on standard error when no Message has it. There is
no daemon: `create`, `show` and `list` each open the Inbox directly.

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
