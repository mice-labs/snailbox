# snailbox
Snails in one place

## Usage

```sh
id=$(snailbox create --from alice --to bob --to carol --subject 'lunch?' --body 'Noon?')
snailbox show "$id"          # readable text, creation time in local time
snailbox show "$id" --json   # JSON, creation time in RFC 3339 UTC
snailbox create --from bob --to alice --subject 're: lunch?' --reply-to "${id:0:8}"
```

`create` prints only the new Message's full id, a random UUID version 4. `--reply-to` stores
the Message as a Reply to the named Message, and `show` prints a Reply's parent id (a
`Parent:` line in text, `parent` in JSON, null when the Message is not a Reply). `show` and
`--reply-to` take a Message's id or any non-empty prefix of it, in either letter case, that
matches exactly one Message; they exit non-zero with an error on standard error when the
prefix is empty or no Message matches, or, listing the matches, when several do. There is no
daemon: `create` and `show` each open the Inbox directly. `list` is not implemented yet.

The Inbox is a SQLite database in the folder `SNAILBOX_HOME` names when it is set, else
`$XDG_DATA_HOME/snailbox/` when that is set, else `~/.local/share/snailbox/`. An empty
variable counts as unset. `create` or `show` creates a missing folder, readable and
writable by its owner only; a folder that already exists keeps its permissions.

## Development

`cargo build` builds the `snailbox` binary; `snailbox --help` lists its commands.
`make check` runs the formatter check, the linter (warnings fail it), the tests and the
Markdown checks.
