# kitty configuration for venti

venti 0.1.0 | 2026-11-16 | linux only

## let venti see alt chords

kitty binds many alt+ chords by default: `alt+1..9` go to tabs, `alt+enter`
toggles fullscreen, `alt+minus`/`alt+equal` change font size. those never
reach venti unless you unbind them in `kitty.conf`. unbind the ones venti
uses, e.g.:

```conf
map alt+1 no_op
map alt+2 no_op
map alt+3 no_op
map alt+4 no_op
map alt+5 no_op
map alt+6 no_op
map alt+7 no_op
map alt+8 no_op
map alt+9 no_op
map alt+enter no_op
map alt+n no_op
map alt+w no_op
map alt+q no_op
map alt+space no_op
```

you only need to unbind what you actually use. venti binds the entire alt+ row
and f1-f12; f-keys are always delivered.

note: venti never binds `alt+tab`, it is the window manager's switcher.

## images and scrolling

kitty images inside a pane scroll with the text they belong to: scroll far
enough and the image is deleted from the host grid, then re-placed when its
rows come back into view. an image clipped by the pane edge is re-placed
cropped to the visible rows instead of vanishing. ED2 repaints (resize,
workspace switch, a full-screen app exiting) re-transmit or re-place the
images so nothing disappears permanently; a transmission that reads its
pixels from a file is re-placed rather than re-transmitted, because the app
deletes the file right after the first send.

## mouse

venti's mouse reporting (wheel scroll, click focus, drag resize, drag-swap)
works out of the box; kitty's default `mouse_map`s are fine.
