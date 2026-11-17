# terminal graphics

venti captures graphics sequences emitted by pane processes and forwards them
to attached clients. it does not decode image pixels or render them itself.

## support status

| protocol | implemented behavior | verified behavior | limitations |
|---|---|---|---|
| kitty graphics | parses apc commands, assembles bounded chunks, maps image ids across panes, tracks placements, and re-emits or crops some placements after redraws | parser, ownership, resize, scroll and protocol-forwarding tests pass | visible output has not been tested against a terminal matrix; several advanced actions are not implemented |
| sixel | captures dcs sixel sequences, estimates dimensions, tracks cursor rows and forwards the sequence | parser, cursor movement and protocol-forwarding tests pass | venti does not decode pixels or implement terminal clipping; correct scrolling and redraw depend on host behavior |
| iterm2 inline images | captures osc 1337 file sequences and forwards them to clients identified as compatible | parser coverage only | dimensions, file metadata, cursor movement, clipping and visual lifecycle are not fully implemented or verified |

“forwarding tests pass” means the test observed a graphics command in a client
screen update. it does not prove that a terminal displayed the image correctly.

## kitty placeholders

kitty's unicode placeholder mode uses placeholder cells in the text grid to
anchor virtual placements. venti recognizes the placeholder encoding and
rewrites pane-local image ids when it forwards image data. correctness across
reflow, selection, and every combination of wide or combining characters has
not been verified in real terminals.

## capability selection

graphics capabilities are currently selected from terminal names and a small
set of environment variables. tmux forwarding uses its passthrough envelope
when the outer terminal can be identified. this is best effort: venti does not
perform a live capability handshake with the host, and unknown or nested
multiplexers may not forward graphics.

## tests

run the graphics-focused tests with:

```sh
cargo test --test graphics_protocol_test --test graphics_conformance_test --test sixel_graphics_test
```

the conformance tests invoke `kitten` and `chafa` when installed. they check
that graphics payloads stay out of pane text and reach the client update. they
do not inspect a rendered terminal image. tests that need a missing executable
skip that protocol case.

## known gaps

- no actual-terminal compatibility matrix has been run for kitty, ghostty,
  wezterm, foot, konsole, iterm2, tmux, or zellij.
- sixel and iterm2 graphics are forwarded as protocol bytes, not represented
  as decoded image resources with protocol-independent clipping.
- capability replies can only describe the terminal identity venti detected;
  they are not based on a live host response.
- animation, complete kitty deletion semantics, and every kitty transfer mode
  have not been implemented and tested end to end.
