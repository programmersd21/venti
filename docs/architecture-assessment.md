# architecture assessment (2026-11-17)

## terminal emulation

venti uses `term-wm-vt100` (a patched vt100 crate) for terminal emulation.
each pane owns a `vt100::Parser` behind an `Arc<Mutex<>>`
(`src/pane/emu.rs:453`). output from the pty reader thread is split by
`GraphicsParser` into plain and graphics segments, then fed to the parser.

manual reflow lives in `src/pane/emu.rs` (`reflow_on_resize`, `wrap_line`).
cursor blink sequences (DECSET 12) are stripped at the reader
(`remove_cursor_blink_sequences`). cursor shape (DECSCUSR) is partially
handled: the client tracks `cursor_shape` in `ApplyState` and re-asserts it
after a DECSTR soft reset (`src/client/apply_update.rs:262`).

weakness: the vt100 crate does not expose enough state for correct reflow
under all conditions. the manual reflow is a workaround, not a fix.

## per-pane state

cursor position, scrollback, alternate screen, and mouse reporting are
per-pane (`Pane` struct in `src/pane/emu.rs`). cursor shape is tracked
client-side in `ApplyState`, not per-pane on the server. terminal modes
(mouse reporting, bracketed paste) are stored in `Arc<AtomicBool>` per pane.

## graphics

`GraphicCommand` (`src/pane/graphics.rs:246`) is the canonical representation.
`PaneGraphicsStore` owns placements with LRU eviction, scroll tracking,
and file-backed transmission caching. protocol parsing (kitty APC, sixel DCS,
iterm2 OSC 1337) is in `GraphicsParser`.

weakness: the canonical model is still kitty-centric. sixel and iterm2
commands are stored verbatim. there is no conversion between protocols.

## client rendering

the client applies screen updates via `apply_update.rs`. graphics are
forwarded verbatim based on `OuterGraphics` capability detection
(`src/util/outer_term.rs`). there is no client-side image state; the host
terminal owns the pixels.

## git history

all commits in this repository were created in a single session on
2026-11-16/17 with backdated timestamps. the 29-commit history does not
reflect organic development. this is documented honestly here rather than
being papered over.

## root causes

1. terminal emulation relies on a patched vt100 crate with manual reflow.
   replacing it with `alacritty_terminal` or `wezterm-term` would be a
   multi-week migration requiring rewriting most of `src/pane/emu.rs` and
   `src/server/core.rs`.

2. graphics forwarding depends on the outer terminal remembering placements.
   venti owns the store but forwards raw sequences, creating a fragile
   dependency.

3. cursor shape is tracked client-side, not per-pane on the server, so
   focus changes can leave stale cursor state.

4. the pet feature and 15 themes add maintenance burden without strengthening
   core reliability.

## recommendations

- evaluate `alacritty_terminal` for terminal emulation (VT compatibility,
  scrollback, reflow, alternate screen, cursor state, modes).
- move cursor shape into per-pane state.
- keep the canonical graphics model but decouple from kitty identifiers.
- remove or optionalize the pet feature.
- be honest about git history (documented here).
