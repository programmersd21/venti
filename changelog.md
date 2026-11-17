# changelog

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
