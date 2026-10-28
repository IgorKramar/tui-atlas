# Catalog data contract

**English** · [Русский](data-contract.ru.md)

The catalog is compiled into the binary. Edit the JSON files and rebuild; Atlas does not load catalog overrides at runtime.

## Inventory

`data/inventory.json` is an array with one entry per app. The runtime reads these required string fields:

| Field | Meaning |
|:---|:---|
| `id` | Stable app identity, for example `nvim`; must match one guide |
| `name` | Display name |
| `group` | Display category |
| `version` | Version recorded in the curated snapshot; not detected at runtime |
| `executable` | Executable name or known path; `~/` expands to the current home directory |

The existing `command` and `source` fields are inventory provenance; the runtime does not use them. Names without `/` are resolved through `PATH`. Paths containing `/` are checked directly; there is no `PATH` fallback for a missing recorded path. Only a regular executable file is considered available.

## Guides

Each `data/guides-*.json` is an array of guide objects. One guide per inventory ID; duplicate guides, missing guides, and guides without an inventory entry are rejected.

| Required field | Type and meaning |
|:---|:---|
| `id` | String; matches the inventory identity |
| `description` | String; app purpose in the Russian source catalog |
| `launch_args` | Array of default argument strings, without the executable |
| `help_args` | Array of safe help argument strings, or `null` when no separate safe help command exists |
| `man_pages` | Array of local man page names |
| `doc_files` | Array of objects with string `path` and `label` |
| `shortcuts` | Array of objects with string `key`, `action`, and `context` |
| `examples` | Array of objects with string `command` and `description` |
| `notes` | Array of strings |
| `sources` | Array of objects with string `title` and `location` |

`context` identifies the screen, mode, or local override. Distinguish standard bindings from observed personal configuration. Examples are reference text; the launch dialog edits only the arguments of the selected executable.

Use verified local man pages, documentation, safe help output, or official upstream sources. Do not invent key combinations. Record relevant versions and describe platform or configuration differences.

## English translations

The Russian source strings are translated through the flat string maps in `data/catalog-en-a.json`, `data/catalog-en-b.json`, and `data/catalog-en-c.json`. Keys must be unique across all three maps. The split is for maintenance; it does not correspond to individual guide files.

Translate human-readable inventory names/groups, guide descriptions, shortcut keys/actions/contexts, example commands/descriptions, notes, document labels, and source titles. Command examples may translate placeholders, but **keep executable names and literal flags unchanged**. Keep real key bindings unchanged; translate explanatory key labels only.

Do not translate IDs, versions, executable paths, launch/help arguments, man page names, document paths, or source locations. Search indexes both the source and translated display strings. Unmapped strings fall back to the source; the test suite requires an English translation for every curated display string containing Cyrillic characters.

## Local files and safety

`doc_files.path` may start with `~/`. Include only documentation and known keybinding files, never authentication profiles, token stores, private keys, or work-access files. Missing local sources are shown as unavailable; they are not downloaded. Atlas also checks a small explicit set of personal keybinding files for tmux, Yazi, nchat, and Neovim when present.

Viewing a CLI tab can execute the configured help command. Keep `help_args` noninteractive and free of writes or network operations; use `null` if that cannot be assured. The user explicitly confirms launch arguments before the full program is started. Defaults such as `cfdisk --read-only` and K9s `--readonly` are intentional.

## Validate a change

```sh
cargo fmt --all -- --check
cargo clippy --locked --all-targets -- -D warnings
cargo test --locked
cargo build --release --locked
python3 tests/terminal_smoke.py target/release/tui-atlas
```

Tests check catalog coverage, translation coverage, and preservation of command identities, flags, and source paths. Inspect both languages and confirm long text fits wide and narrow layouts. Update both READMEs if the card/shortcut/example totals change.
