# graphics and unicode placeholders

venti 0.1.0 | 2026-11-16 | linux only

venti forwards sixel, kitty, and iterm2 graphics escape sequences from panes to
clients. this page describes how kitty's unicode placeholder (u=1) mode works
with venti.

## what venti does

- **capability probe:** panes answer kitty graphics protocol queries (`a=q`)
  with `e=t,u=1` when the outer terminal can do kitty graphics, so applications
  (yazi, lf) enable their placeholder path. when the outer terminal can't,
  venti answers with `ENOENT:kitty graphics not available` and the app falls
  back to non-graphics rendering.
- **parsing:** the splitter is byte-exact like zellij's APC filter: an `ESC`
  always starts a sequence (even mid-utf-8 character) and `CAN`/`SUB` abort a
  transmission in progress. a transmission can therefore never fall through to
  the text stream, where its payload would be printed as garbage.
- **passthrough:** kitty apc, sixel dcs, and iterm2 osc 1337 sequences written
  by the app are forwarded to the outer terminal. inside tmux, sequences are
  wrapped in tmux's dcs passthrough envelope (`allow-passthrough on` required);
  anything the outer terminal can't render is dropped rather than printed as
  garbage.
- **anchoring:** kitty *virtual* placements (`u=1`) are text-anchored to
  u+10eeee placeholder cells the app paints into the grid. those cells flow
  through the normal text path, so images scroll with content for free.
- **cell re-id:** venti transmits each image under a daemon-wide id so pane-local
  ids from different apps can't collide. placeholder cells carry the app-local
  id in their foreground color; venti rewrites that color (and the optional
  msb diacritic) to the daemon-wide id on render so the host anchors the
  placeholders to an image that was actually transmitted.
- **real placements:** for ordinary kitty transmissions, venti re-emits the
  placement (delete + `a=p` referencing the stored image) after redraws,
  resizes, and reveal-from-hidden so images don't vanish when panes repaint.
  when an image is only partially visible (clipped by the top or bottom pane
  edge after scrolling), venti re-places it cropped to the visible source
  rectangle instead of hiding or overdrawing its off-screen rows.
- **scrolling:** image anchor rows are shifted when content scrolls. images
  scrolled entirely out of view are deleted from the host grid and re-placed
  when they become fully visible again; images clipped by the pane edge are
  re-placed cropped to the visible fragment.
- **resize:** a pane resize reflows the grid under the images. the placements
  stay in the pane's store and are re-placed (cropped to the new viewport)
  by the repaint that follows, so a logo drawn before the resize is still
  there afterwards.
- **full-screen apps:** when a TUI (yazi, less, btop) exits the alternate
  screen, the placements it left behind are hidden on the host, not deleted
  from the store: the image data stays and the placement is re-created when
  the pane returns to the main screen. the same holds when a foreground app
  exits: its placement is hidden (delete on the host), the pixels stay, and
  the next repaint re-places it.
- **clipping:** an image bigger than its pane is not hidden but clipped to
  it: the placement is re-issued with the source rectangle cut down to the
  visible cells (zellij clips an image quad to its owning pane the same way).
  `kitten icat` of a large screenshot therefore shows the part that fits.
  the crop uses the placement's own scale (source pixels per cell), not the
  terminal's cell size - an app may scale the image into fewer cells than
  its pixel size suggests.
- **file transmissions:** a transmission that reads its pixels from a file
  (`t=f`, `t=s`) is read once at capture time and cached. the app deletes
  the file right after the transmission, so the command is rebuilt as an
  inline `t=d` transmission before the first frame reaches the host. every
  inline payload is chunked at the protocol's 4096-byte base64 limit with
  `m=1`/`m=0`, exactly like the app's own transmission was.
- **scrollback anchoring:** a placement that already scrolled into the
  scrollback keeps its distance to the live grid (new output is appended
  below it), so images stay with their text instead of drifting upwards.
- **history eviction:** a placement anchored further above the grid than the
  retained scrollback is gone for good; it is dropped from the store and the
  host is told to release the pixels. the store evicts the least recently
  used placement first when its byte budget is exceeded, so images on screen
  are never the ones dropped.

## protocol maturity

| protocol | tested | notes |
|---|---|---|
| kitty graphics | most tested and most stable | every code path is covered by unit tests plus the real-tool conformance suite; `kitten icat`, yazi and fastfetch all work |
| sixel | weaker | parse, render, scroll work; needs a sixel-capable outer terminal |
| iterm2 inline images | not well tested | display only |

kitty is the protocol to rely on. sixel is the fallback for terminals without kitty support, and iterm2 is best-effort.

## what venti does not do

- venti does not rewrite the app's `a=t` transmissions into `u=1` virtual
  placements. apps opt in by observing the capability probe response.
- scrollback is a terminal buffer feature: images are anchored to grid cells,
  and venti keeps out-of-view commands instead of deleting them.

## resizing and reflow

the pane grid is reflowed when the pane gets narrower (a float toggle, a
split change): the vt100 grid truncates rows to the new width, so the visible
text is captured before the resize and written back wrapped to the new width,
with the cursor restored. zellij reflows its grid the same way.

graphics anchored in the grid follow their content through that reflow:

- a pane that shrinks in height scrolls the rows that no longer fit into the
  scrollback; the placements in those rows are shifted by the scrolled amount
- the re-wrap moves the remaining rows; the placements are re-mapped through
  the same row mapping

so an image ends up where its text is: not left on the old row (drawn over
the pane border or outside the pane) and not drawn twice (the stale copy plus
the re-placed one). a sixel has no delete sequence, so its re-placement always
rides on the full ED2 repaint. an image that scrolled out of view is deleted
from the host and re-placed when it is scrolled back into view.

## probing

apps detect the outer terminal by asking it questions. venti answers the ones
that matter: primary device attributes (claiming sixel and kitty flags only
when the outer terminal actually supports them), the graphics capability probe
(`a=q`), cursor position and size in cells and pixels, and the osc 10/11/12
default-color queries. without the color answers chafa blocks for seconds and
then gives up without drawing anything, so they are answered from the theme.

## delivery

a placement is delivered per client, not per session:

- every client keeps its own set of placements it has received and of
  placements it has seen hidden. a client that attaches later, or one that
  missed a frame, still gets the image (and the pixels, not just a
  re-placement of data the host may have dropped)
- every emission carries a fresh session-wide sequence. the client drops
  commands that are not newer than the last one it saw, so a re-placement
  always has to look fresh
- a frame always follows a graphics-only output chunk: the pane marks itself
  dirty when a command is stored, so the image cannot be painted into a frame
  that is never sent

## testing

`cargo test graphics` covers the parser, store, and placeholder rewriting.
`tests/graphics_conformance_test.rs` runs the real tools (`kitten icat`,
`chafa -f sixels`) against a real server and checks that the payload never
reaches the text stream, that the image reaches the client, and that every
sequence is terminated.

graphics is experimental. it is on by default; if images glitch on your
terminal, pass `graphics = false` in `[general]` to disable forwarding entirely.
