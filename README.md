# kanban-bridge

Every kanban tool exports boards in its own shape. Trello gives you a single
JSON blob with flat `lists` and `cards` arrays linked by id. Plenty of other
tools (note-taking apps, static site generators, plain text setups) expect a
simple markdown checklist instead:

```markdown
# Sprint 12

## To Do

- [ ] Write the migration script
- [ ] Review PR #482

## Doing

- [ ] Fix flaky login test

## Done

- [x] Cut the release branch
```

`kanban-bridge` converts between the two so you can move a board out of
Trello into a text file (or the other way around) without hand-editing JSON.

## Usage

Build it with plain `cargo` - no dependencies to fetch:

```
cargo build --release
```

Convert a Trello export to markdown:

```
./target/release/kanban-bridge to-md trello-export.json > board.md
```

Convert markdown back into a Trello-shaped JSON export:

```
./target/release/kanban-bridge to-trello board.md > board.json
```

Both commands read from stdin when you omit the file argument (or pass `-`),
so they compose with other tools:

```
cat trello-export.json | kanban-bridge to-md > board.md
```

## Format notes

- A list named "Done" (case-insensitive) converts to checked boxes; every
  other list converts to unchecked boxes. Trello itself has no concept of a
  checked card, so this is a naming convention, not a Trello field.
- Archived lists and cards (`"closed": true` in Trello) are dropped when
  converting to markdown.
- Converting markdown to Trello JSON fills in only the fields a real Trello
  export always has (`name`, `lists`, `cards`, `id`, `idList`, `closed`).
  Fields Trello also writes, like card descriptions, due dates, labels and
  members, are not read or produced yet.

## License

MIT, see LICENSE.
