# changelog

## [0.2.0] - 2026-11-19

major architecture release: sixel and kitty graphics protocol (kgp) support, unicode placeholders, and wayland terminal hardening.

### added

- **pure-rust graphics pipeline**: powered by `icy_sixel` v0.7 and `image` with zero c dependencies.
- **dual protocol pass-through**: transparent, high-performance forwarding for both sixel (`ESC P ... q ... ESC \`) and kitty graphics protocol apc (`ESC _G ... ESC \`) sequences.
- **kitty unicode placeholders (`U+10EEEE`)**: images are anchored to virtual character cells with diacritics, keeping images locked to grid cells across splits, resizing, and scrolling.
- **pty stream interceptor**: graphic escape sequences are stripped from incoming child pty streams before reaching `vt100::Parser`, eliminating terminal text corruption.
- **viewport culling & boundary clipping**: sixel payloads are bit-masked and row-truncated to pane boundaries; off-viewport images are culled to prevent frame bleed.
- **layered rendering order**: graphics sequences are emitted before character cell matrices, guaranteeing text overlays, preview borders, and modal menus paint cleanly on top without tearing.
- **outer terminal capability auto-detection**: automatic detection for foot, ghostty, kitty, wezterm, and konsole via `da1` and `xtversion` query responders.
- **host cleanup on client exit**: automatic deletion of forwarded kitty image placements on detachment, leaving host shells completely clean.

### changed

- updated `Cargo.toml` dependencies with pure-rust `icy_sixel` and `image` crates with minimal feature sets.
- streamlined documentation suite into clean, lowercase, structured guides across all core topics.
- bumped version to `0.2.0`.

### fixed

- detects the attached terminal from both `TERM` and `TERM_PROGRAM`, preserving kitty and sixel capabilities when `TERM` is generic.
- follows alternate-screen grid scrolling when positioning graphics and drops sixel placements when new text overwrites their cells.
- keeps kitty and sixel graphics in the client update path across redraws and resize events.
- preserves already-encoded kitty payloads when re-chunking chafa images.
- forwards kitty animation feature probes and chunked animation frames.
- schedules a final graphics repaint when pane resize animations finish.
- fixed sixel image tearing and ghosting when navigating folders and previews in yazi.
- fixed text cells corrupting underlying graphics layers by rendering graphics before text cells in `apply_update`.
- fixed scrollback stacking where scrolling up caused sixel images to repeatedly re-paint over previous frames.

---

## [0.1.0] - 2026-11-16

first stable release. linux only.

### added

- persistent detachable sessions over unix domain sockets.
- dwindle tiling + floating panes, workspaces, scratchpad, zoom.
- copy mode, search, command palette, vim `:` commands.
- settings menu (f2), help (f1), sidebar, theme picker (f12).
- four keybinding profiles: beginner, hypr, vim, dev.
- 15 dark themes, live config editing, hot reload.
- sgr mouse, kitty keys, bracketed paste.
- workspace tab context menu, scroll indicator, sidebar fold.
- animations, cli help, shell completions.

### fixed

- kitty payload leak into text stream.
- kitty chunking at 4096 bytes; file-backed transmissions cached inline.
- pane resize reflows text; graphics follow reflow.
- command line accepts spaces in arguments.
- right-click tab opens menu; elsewhere no-op.
- osc 10/11/12 color queries answered without blocking chafa.
- kitten icat temporary file caching support.

[0.2.0]: https://github.com/programmersd21/venti/releases/tag/v0.2.0
[0.1.0]: https://github.com/programmersd21/venti/releases/tag/v0.1.0
