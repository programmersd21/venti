# roadmap

venti 0.1.0 | 2026-11-16 | linux only

## v0.1 (current, stable)

baseline complete:
- persistent sessions, tiling and floating layouts, workspaces
- copy mode, scrollback search, vim `:` command mode
- terminal graphics (kitty, sixel, iterm2): experimental, see the readme
- mouse support, 15 dark themes, live settings editor
- four keybinding profiles, layout animations
- comprehensive test suite

## v0.2 (next)

stability and polish:
- [ ] session recovery after a crash (currently the layout file restores, the panes do not)
- [ ] better error messages for terminal capability failures
- [ ] per-workspace keybind overrides
- [ ] notifications for bell and background task completion
- [ ] structured logging
- [ ] performance profiling

## v0.3

features:
- [ ] window groups (tags)
- [ ] monocle improvements
- [ ] themes loaded from a file (built-in themes and color overrides work today)
- [ ] hooks (on events)
- [ ] conditional keybindings

## v0.4+

future:
- [ ] scripting api (control via shell)
- [ ] clipboard manager integration
- [ ] spawn from templates
- [ ] recording/playback
- [ ] experimental federation

## non-goals

**not planned:**

- gui (terminal-only forever)
- network (local unix domain sockets only)
- per-pane config (violates simplicity)
- embedded scripting (call external)
- tmux/screen compatibility (own thing)

## priorities

1. **stability** - crashes are unacceptable
2. **responsiveness** - latency matters
3. **simplicity** - fewer features, done well
4. **correctness** - before micro-optimizations

## feedback

suggest ideas in issues. we read them.
