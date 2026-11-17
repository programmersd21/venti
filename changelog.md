# changelog

## [0.2.0] - unreleased

### added

- architecture assessment document
- osc 10/11/12 color query responder (chafa no longer blocks)
- csi 14/16/18t pixel-size answers for kitten icat
- kitty chunking at 4096 bytes (m=1/m=0)
- file-backed kitty transmissions cached and rebuilt inline
- help overlay scroll indicator as proportional thumb
- all-command parse regression test
- catppuccin-themed badges with logos in readme
- github labels, discussions, topics, funding.yml
- `alt+b` workspace bar toggle (later removed per user request)

### changed

- graphics off by default (experimental)
- readme: honest protocol maturity matrix (kitty most tested, sixel weaker, iterm2 untested)
- docs: lowercase, no em dashes, consistent with codebase

### fixed

- kitty payload leak into text stream
- `:` command line accepts spaces in arguments
- right-click tab opens menu, elsewhere no-op
- per-client graphics delivery, fresh session-wide seq per emission
- settings-menu apply failures report error instead of "updated"
- failed frame draw drops client instead of pinning session
- floating resize tests: mirror session-wide sets from per-client sets
- `:` command line shows real parse error instead of generic "unknown command"

### removed

- dead `ClearScanner`, `dbg_*` tests, unused `insta` dependency
- `alt+b` / `ToggleWorkspaceBar` (user request: only `shift+tab`)

## [0.1.0] - 2026-11-16

first stable release. linux only.

### added

- persistent detachable sessions over unix domain sockets
- dwindle tiling + floating panes, workspaces, scratchpad, zoom
- copy mode, search, command palette, vim `:` commands
- settings menu (f2), help (f1), sidebar, theme picker (f12)
- four keybinding profiles: beginner, hypr, vim, dev
- 15 dark themes, live config editing, hot reload
- sgr mouse, kitty keys, bracketed paste
- workspace tab context menu, scroll indicator, sidebar fold
- animations, cli help, shell completions

### changed

- terminal graphics (kitty, sixel, iterm2) are experimental
- graphics off by default; set `graphics = true` to try

### fixed

- kitty payload leak into text stream (esc wins over utf-8, can/sub abort)
- kitty chunking at 4096 bytes, file-backed transmissions cached inline
- pane resize reflows text, graphics follow reflow
- `:` command line accepts spaces in arguments
- right-click tab opens menu, elsewhere no-op
- per-client graphics delivery, fresh session-wide seq per emission
- osc 10/11/12 color queries answered (chafa no longer blocks)

### removed

- dead `ClearScanner`, `dbg_*` tests, unused `insta` dependency

[0.1.0]: https://github.com/programmersd21/venti/releases/tag/v0.1.0
