# wire protocol

venti uses a binary length-delimited ipc wire protocol over unix domain sockets. messages between the client and server are serialized using `postcard`.

## framing

every message on the unix socket has a 4-byte little-endian length prefix followed by the postcard-encoded payload:

```
+-----------------------------------+-----------------------------------+
|   length: u32 (little endian)     |   postcard serialized message     |
|   [ 4 bytes ]                     |   [ length bytes ]                |
+-----------------------------------+-----------------------------------+
```

maximum payload size is 32 MiB (`MAX_FRAME_SIZE`). the decoder checks the length
before allocating payload storage and rejects oversized frames.

## handshake

1. client connects to `$XDG_RUNTIME_DIR/venti/<uid>/<session>.sock`.
2. client sends `ClientMsg::Hello`:
   - `protocol`: protocol version number
   - `term`: terminal identifier (e.g. `foot`, `xterm-256color`)
   - `env`: currently includes the host `TERM_PROGRAM` identity when set
   - `colorterm`: truecolor support flags
   - `size`: initial terminal dimensions `(cols, rows)`
   - `cell_pixel`: pixel size of individual character cells `(width, height)`
3. server responds with `ServerMsg::HelloAck`:
   - `session`: session name confirmation
   - `graphics`: capability bitmask detected from the attached client's `TERM` and `TERM_PROGRAM`

## message types

### client to server (`ClientMsg`)
- `Hello(HelloMsg)`: initial connection parameters.
- `Input(Vec<u8>)`: raw keyboard and terminal input sequences to route to the active pane.
- `Resize(u16, u16)`: window size change notification.
- `Command(String)`: named multiplexer commands dispatched from keybindings or cli (`venti run`).
- `Detach`: clean client detachment.

### server to client (`ServerMsg`)
- `HelloAck { session, graphics }`: successful connection confirmation.
- `Update(ScreenUpdate)`: incremental screen update containing:
  - `cols`, `rows`: active grid dimensions
  - `cells`: vector of `CellDiff` (dirty characters, styles, and colors)
  - `gfx`: vector of `GfxCmd` (graphic sequences and placement coordinates)
  - `cursor_pos`, `cursor_visible`, `cursor_shape`: cursor state
  - `full_redraw`: clear screen flag
- `Exit`: server shutdown notification.
