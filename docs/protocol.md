# protocol

venti 0.1.0 | 2026-11-16 | linux only

linux only. binary protocol over local unix domain sockets. protocol version 1.

## frame format

```
┌─────────────┬─────────────────────┐
│ 4 bytes     │ n bytes             │
├─────────────┼─────────────────────┤
│ length (le) │ postcard payload    │
└─────────────┴─────────────────────┘
```

- **length:** u32 little-endian (payload size; the length prefix itself is excluded)
- **payload:** postcard-encoded message
- **max frame:** 32 mib (scrolled-out images legitimately exceed 1 mib)

## messages

### handshake

client → server, once per connection:

```rust
hellomsg {
    protocol: u16,            // protocol_version (1)
    term: string,             // term env
    colorterm: option<string>,
    size: (u16, u16),         // cols, rows (clamped to 1..=1000 x 1..=500)
    env: vec<(string, string)>,
    cell_pixel: option<(u16, u16)>, // pixel size of one cell, when the terminal reports it
}
```

clients with a mismatched `protocol` are disconnected before registration.

### client messages

```rust
enum clientmsg {
    hello(hellomsg),
    input(vec<u8>),           // raw bytes from the keyboard
    mouse(mouseeventmsg),     // sgr-encoded mouse reports
    resize(u16, u16),         // terminal resized (clamped to 1..=1000 x 1..=500)
    paste(vec<u8>),           // pasted bytes (bracketed paste wrapped by server)
    command(string),          // `venti run`/`kill`/`paste` one-shot commands
    detach,                   // close this client
}
```

### server messages

```rust
enum servermsg {
    helloack { session: string },
    update(screenupdate),
    settitle(string),         // set the outer terminal's window title
    clipboard(vec<u8>),       // osc 52 payload, already base64-encoded by the client
    exit(exitreason),         // detached | killed | error(string)
    rawoutput(vec<u8>),       // raw bytes for the outer terminal (osc 4/10/11)
}
```

## screen update

```rust
screenupdate {
    cols: u16,
    rows: u16,
    cursor_pos: option<(u16, u16)>,
    cursor_visible: bool,
    cursor_shape: cursorshape,     // default | block | underline | bar
    cells: vec<celldiff>,
    full_redraw: bool,             // clear the host screen before painting
    gfx: vec<gfxcmd>,
    reset: bool,                   // client emits decstr (csi ! p) before painting
}

celldiff {
    x: u16, y: u16,
    symbol: string,
    fg: protocolor,                // default | rgb(u8,u8,u8) | indexed(u8)
    bg: protocolor,
    bold: bool, dim: bool, italic: bool,
    underline: bool, reverse: bool, strike: bool,
}

gfxcmd {
    row: u16, col: u16,
    format: gfxformat,             // sixel | kitty | iterm2
    seq: u64,                      // daemon-wide monotonic id
    data: vec<u8>,
}
```

cells are emitted before graphics so images paint on top. clients only
receive diffs; `full_redraw` forces a full ed2 clear first.

## notes

- `mouseeventmsg` carries `button`/`action`/`shift`/`alt`/`ctrl`; coordinates
  are 0-based cell positions.
- graphics data is opaque to venti: sequences written by pane applications are
  forwarded verbatim (when the outer terminal can render them); the client
  terminal interprets them.
- `reset` is set when the terminal needs a decstr soft reset; the client emits
  `csi ! p` before painting and re-asserts mouse capture modes afterwards.
- `gfx.seq` is a daemon-wide monotonic id. the client drops a command whose seq
  is not newer than the last one it applied, so a re-placement always looks fresh.
