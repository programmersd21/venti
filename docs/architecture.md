# architecture

venti 0.1.0 | 2026-11-16 | linux only

linux only. client-server architecture over local unix domain sockets.

## system overview

```
┌──────────────────────────┐
│ client process           │
│ • terminal i/o           │
│ • input capture (raw)    │
│ • render screen updates  │
│ • socket communication   │
└────────────┬─────────────┘
             │ binary (postcard)
             │ unix socket
┌────────────▼─────────────┐
│ server daemon            │
│ (one per xdg_runtime_dir)│
│ • session manager        │
│ • pane/workspace tree    │
│ • pty allocation         │
│ • terminal emulation     │
│ • layout engine          │
│ • event loop             │
└──────────────────────────┘
```

## client

**captures input** → sends to server → receives screen delta → renders.

**flow:**
1. connect to the session socket, `venti new` starts the daemon if it is missing
2. send handshake (term type, size, cell pixel size)
3. loop:
   - capture keyboard/mouse
   - send to server
   - read screen update
   - render diff

**detach:** close socket or timeout. session continues running.

## server

**manages sessions, ptys, layout, rendering.**

**core loop:**
```
loop {
  1. wait for events (input, pty, timer)
  2. process input → actions (split, focus, etc)
  3. layout changes → animation setup
  4. pty output → terminal emulation → screen buffer
  5. render:
     - if animating: interpolate rects
     - compute pane positions
     - emit deltas to clients
  6. broadcast updates
}
```

## session

each session owns:
- multiple workspaces
- persistent layout (tree of splits)
- connected clients
- panes (each owns pty + child process)
- scrollback buffers

**state:**
- socket: `$xdg_runtime_dir/venti/<uid>/<name>.sock`
- layout: `$xdg_state_home/venti/sessions/<name>.toml` (persisted)

## layout

**tree structure:**
```
workspace
  └─ layoutnode (split)
      ├─ layoutnode (split)
      │   ├─ pane (tiling)
      │   └─ pane (tiling)
      └─ floatingpane (xy, size)
```

**modes:**
- **dwindle:** recursive binary splits, aspect-aware
- **monocle:** fullscreen per pane

**floating:** independent position/size, drag/resize with mouse.

## pane

each pane owns:
- pty (pseudo-terminal)
- child process
- vt100 terminal state
- scrollback buffer
- graphics metadata
- selection (copy mode)

**vt100 emulation:** patched term-wm-vt100.

## protocol

**handshake:**
```
client → server: hellomsg (protocol, term, colorterm, size, env, cell_pixel)
server → client: helloack { session } followed by the first screen update
```

**client → server:** `input`, `mouse`, `resize`, `paste`, `command`, `detach`

**server → client:** `helloack`, `update`, `settitle`, `clipboard`, `exit`, `rawoutput`

**screen update (delta):**
```
screenupdate {
  cells: vec<celldiff>,      // only changed cells
  graphics: vec<gfxcmd>,
  cursor_pos: option<(col, row)>,
  cursor_visible, cursor_shape,
  full_redraw,               // client clears its screen first
  reset,                     // client emits decstr before painting
}
```

**codec:** postcard (efficient binary), length-prefixed.

## rendering

1. compute layout (rects per pane)
2. if animation: interpolate rects
3. emit render commands
4. collect changed cells
5. send delta to client

**differential:** only cells that changed → low bandwidth.

## animation

**setup:** on layout change, create animation state (320ms, ease_out_cubic).

**execution:** interpolate pane rects every frame.

**completion:** use final rects when elapsed >= duration.

## concurrency

**single-threaded core loop** (no async runtime).
- all state mutations in the main loop
- per-pane reader and writer threads; vt100 parser state behind a mutex
- pty/child events flow to the core over a bounded channel
- signal handler: only an atomic flag
- config file watcher: notify events

## configuration

**persistence:**
- config: `~/.config/venti/config.toml`
- layout: `~/.local/state/venti/sessions/<name>.toml`
- logs: `~/.local/state/venti/venti.log`

**reload:** the config file is watched with notify; the session re-applies the
parts that can change at runtime and writes the result back to the config.

## error handling

**pty failure:** close pane, mark degraded.

**client disconnect:** clean, keep session running.

**protocol error:** close connection, keep session.

**config error:** an invalid config fails fast at load, an empty file fails
loudly, a parse error during a hot reload keeps the last good config and shows
a toast.

## performance

**latency:** single-threaded event loop, diff-based frames; frame interval defaults to 8ms (`ui.frame_interval_ms`)

**throughput:** differential frames only; bandwidth scales with changed cells, not screen size.

**memory:**
- per-pane: vt100 state + scrollback buffer
- binary: ~6mib (release build, 2026-11-16)

## versions

- **protocol:** 1
- **snapshot:** 2
- **rust edition:** 2024
