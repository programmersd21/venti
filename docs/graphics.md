# graphics

venti forwards **sixel** and the **kitty graphics protocol (KGP)**. iTerm2 image sequences are unsupported.

## supported protocols

### sixel
- standard format: `ESC P ... q ... ESC \`
- native on: `foot`, `konsole`, `wezterm`, `contour`, `mlterm`.
- features zero-bloat pure-rust decoding via `icy_sixel`.
- direct bit-masking and row-level truncation ensures images never bleed over pane boundaries or split borders.

### kitty graphics protocol (kgp)
- standard format: apc `ESC _G ... ESC \`
- native on: `kitty`, `ghostty`, `wezterm`.
- forwards Kitty animation frame and playback commands; animated GIFs work with `kitten icat`.
- support for chunked payloads (`m=1` / `m=0`), direct data transmission (`t=d`), and file-backed transfers read by the daemon.
- preserves image data and restores placements after terminal, pane, and animated-layout resizes.
- **unicode placeholders (`U+10EEEE`)**: images are anchored to virtual grid character cells using unicode placeholder characters with diacritics. during split reflows, zooming, and scrolling, images track the pane text accurately without drifting.

---

## behavior and limits

- **pty sequence interceptor**: incoming escape streams from child ptys are parsed in real time off the render thread. graphic escape sequences are stripped from the pty stream before passing through to the `vt100` parser.
- **layered rendering pass**: graphic commands are emitted to the host terminal before drawing text cells, guaranteeing that characters and ui borders paint cleanly over graphic layers without artifacts.
- **strict pane clipping**: sixels are cropped and kitty placements are constrained to active pane rectangles.
- **viewport synchronization**: Kitty placements and Sixel anchors follow observed grid scrolling in primary and alternate screens. Text written over a Sixel placement invalidates that placement before repaint.
- **capability detection**: the client reports both `TERM` and `TERM_PROGRAM`, so generic `TERM` values do not hide Kitty or Sixel support from child applications.
- iTerm2 image protocol and Kitty shared-memory transfer are unsupported.

---

## tool configuration

### yazi
in `~/.config/yazi/yazi.toml`:
```toml
[preview]
image_filter = "lanczos3"
max_width = 1600
max_height = 1200
```
yazi automatically detects sixel or kitty protocol support via query probes answered by venti's pane query responder.

### chafa
to display images using kitty graphics:
```bash
chafa -f kitty image.png
```
or using sixel:
```bash
chafa -f sixel image.png
```

Kitty and Sixel support depends on the attached terminal. iTerm2 image sequences are not forwarded as graphics.

### kitten icat
inside `kitty` or `ghostty`:
```bash
kitten icat image.png
```

---

## troubleshooting

- run `venti doctor` to verify outer terminal graphics detection.
- ensure `general.graphics = true` in `~/.config/venti/config.toml`.
- when running inside tmux outside venti, enable passthrough: `set -g allow-passthrough on`.
