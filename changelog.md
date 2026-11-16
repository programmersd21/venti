# changelog

all notable changes to this project will be documented in this file.

the format is based on [keep a changelog](https://keepachangelog.com/en/2.0.0/),
and this project adheres to [semantic versioning](https://semver.org/spec/v2.0.0.html).

## [0.1.0] - 2026-11-16

first stable release. linux only.

### added

- persistent detachable sessions over unix domain sockets; layout survives restart
- dwindle tiling layout with aspect-aware splits; workspaces; floating panes; scratchpad; zoom
- copy mode with regex search; command palette with fuzzy matching; vim-style `:` commands with autocomplete
- settings menu (f2) with runtime config editing; help screen (f1); sidebar workspace navigation
- four keybinding profiles: beginner (mouse + f-keys), hypr (hjkl), vim (vi grammar), dev (editor ergonomics)
- theme picker (f12) with 15 built-in dark themes
- sgr mouse, kitty keys, bracketed paste
- workspace tab context menu (right-click a tab: new / rename / close)
- `shift+tab` folds the sidebar in every profile
- `decorations.scroll_indicator` (off by default) shows `↑N` in the title while scrolled back
- animations for layout changes; `--profile` switches live via f2
- cli: colored help for every subcommand (`venti help <cmd>`), `venti doctor` capability check
- shell completions for bash, fish, zsh, elvish
- configuration file with hot reload; `venti config init` writes a documented template

### changed

- terminal graphics support is experimental and may be unreliable across terminals, especially during resize, scroll and layout changes. see the readme and `docs/graphics-unicode-placeholders.md`
- graphics is on by default. if images glitch on your terminal, set `graphics = false` under `[general]`; `venti doctor` prints the same reminder
- four distinct profiles replace the old single chord table; f-keys are shared, everything else diverges

### fixed

- kitty payload can no longer fall through to the text stream ("AAAA..." fill): ESC always wins over split utf-8, `CAN`/`SUB` abort a transmission, chunked `m=1`/`m=0` transmissions merge
- the `:` command line now renders even when the hint bar is disabled: it used to share the hint bar's row, so turning the hint bar off made the command prompt invisible
- the `:` command line now shows the real parse error (e.g. "move_to_workspace requires at least one workspace number") instead of the generic "unknown command" for every failure
- kitty images are re-transmitted in spec-compliant chunks: every inline payload larger than 4096 base64 bytes is split into `m=1`/`m=0` chunks instead of one oversized apc the host silently drops
- file-backed kitty transmissions (`t=f`, `t=s`) are cached and rebuilt as an inline chunked `t=d` command at capture time, so the first frame carries the pixels even though the app deletes its temp file immediately (kitten icat was blank)
- the help overlay no longer paints a `░`-shaded track down its right edge; the scroll indicator is a proportional thumb only
- kitty images: re-placement after ED2, scroll anchoring (`row + scroll_offset`), crop-to-pane with the placement's own scale, file-backed (`t=f`/`t=s`) transmissions are re-placed not re-sent, LRU eviction, free-on-scrollback
- sixel: DA1 claims sixel only when the outer terminal supports it; OSC 10/11/12 color queries are answered so chafa and friends stop blocking; DECSDM scrolling; re-placement after resize rides on ED2
- pane resize reflows text instead of truncating it; graphics placements follow the reflow
- the `:` command line accepts spaces: `move_to_workspace 2`, `workspace_goto 2 3`, `rename_workspace my ws`, `close_workspace 2` all work
- right-click on a tab opens the tab menu; right-click anywhere else no longer closes the pane
- `[theme]` color overrides apply on top of the named theme; invalid hex keeps the built-in color with a warning
- `layout.mode` applies immediately on config reload and settings-menu edits
- graphics delivery is per client: a client that attaches later or misses a frame still gets the pixels; every emission carries a fresh session-wide sequence
- a failed frame draw drops the client instead of pinning the session; a clipboard yank reports success only when a client took the payload
- settings-menu edits that fail to apply report the error instead of showing "updated"

### removed

- dead `ClearScanner` screen-clear detector and its test
- `dbg_*` scaffolding tests; `insta` dev-dependency (unused)

[0.1.0]: https://github.com/programmersd21/venti/releases/tag/v0.1.0
