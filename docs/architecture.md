# architecture

venti is a client-server terminal multiplexer designed specifically for linux. it separates stateful session management, pty management, and screen layout into a background daemon, connected to lightweight client frontends over unix domain sockets.

## core architecture

```
+-------------------------------------------------------------+
|                        venti server                         |
|                                                             |
|  +--------------------+             +--------------------+  |
|  |   pty subprocess   |             |   pty subprocess   |  |
|  +--------------------+             +--------------------+  |
|            |                                  |             |
|    (pty reader thread)                (pty reader thread)   |
|            |                                  |             |
|  +--------------------+             +--------------------+  |
|  | vt100 state & gfx  |             | vt100 state & gfx  |  |
|  +--------------------+             +--------------------+  |
|            \                                  /             |
|             +------------+      +------------+              |
|                          |      |                           |
|                    [ session & layout ]                     |
|                              |                              |
|                    [ ratatui render pass ]                  |
|                              |                              |
|                    [ diff-based encoder ]                   |
+------------------------------|------------------------------+
                               | (unix domain socket)
                               v
+-------------------------------------------------------------+
|                        venti client                         |
|                                                             |
|    - applies screen updates with minimal crossterm writes   |
|    - forwards sixel dcs and kitty apc escape sequences      |
|    - captures keyboard and mouse events                     |
|    - streams input back to server daemon                    |
+-------------------------------------------------------------+
```

## key components

### 1. server daemon (`src/server/`)
- owns all active workspaces, layouts, and pane processes.
- handles client connections, socket dispatch, and session lifecycle.
- computes layout geometry (`src/layout/`) with support for dwindle, monocle, and floating panes.
- runs the render pass, computing cell and color diffs between successive frames.

### 2. pty & emulation layer (`src/pane/`)
- launches child shells with `portable-pty` and sets appropriate terminal environment variables (`xterm-kitty` for kitty/ghostty/wezterm, matching outer term for foot/konsole).
- parses escape sequences with a patched `vt100` parser.
- intercepts sixel (`ESC P ... q`) and kitty graphics apc (`ESC _G ... ESC \`) without blocking the main event loop. iterm2 image protocol is unsupported.
- answers capability probes (`da1`, `xtversion`, and kitty `a=q`) truthful to the outer terminal's supported feature set.

### 3. graphics pipeline & unicode placeholders (`src/pane/graphics.rs`, `src/render/pane_view.rs`)
- manages image lifecycles, chunked re-assembly, and dimension caching.
- preserves image ids across kitty animation frames and forwards playback controls.
- uses **unicode placeholders (`U+10EEEE`)** with diacritics mapping daemon-wide image ids to virtual cells.
- enforces strict viewport clipping and scrollback repositioning so images never bleed across pane borders.
- restores kitty placements after completed pane-layout animations and full redraws.

### 4. client frontend (`src/client/`)
- attaches to running sessions via unix domain sockets (`$XDG_RUNTIME_DIR/venti/<uid>/<session>.sock`).
- puts the local terminal into raw mode and alternate screen using `crossterm`.
- emits graphics sequences after cell updates so text and pane borders remain visible above image pixels.
- cleans up forwarded kitty image placements on shutdown so host terminals are left clean.
