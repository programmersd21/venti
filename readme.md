<p align="center">
  <img src="assets/logo.png" alt="venti logo" />
  <br>
  fast, quiet terminal multiplexer for linux.
  <br><br>
  <a href="https://github.com/programmersd21/venti/actions/workflows/ci.yml"><img src="https://img.shields.io/github/actions/workflow/status/programmersd21/venti/ci.yml?style=for-the-badge&logo=githubactions&logoColor=a6e3a1&label=build&color=a6e3a1&labelColor=313244" alt="build" /></a>
  <a href="https://crates.io/crates/venti"><img src="https://img.shields.io/crates/v/venti?style=for-the-badge&logo=rust&logoColor=cba6f7&color=cba6f7&labelColor=313244" alt="crates.io" /></a>
  <a href="https://github.com/programmersd21/venti/blob/main/license"><img src="https://img.shields.io/github/license/programmersd21/venti?style=for-the-badge&logo=opensourceinitiative&logoColor=f9e2af&color=f9e2af&labelColor=313244" alt="license" /></a>
  <a href="https://www.rust-lang.org"><img src="https://img.shields.io/badge/rust-2024-fab387?style=for-the-badge&logo=rust&logoColor=fab387&labelColor=313244" alt="rust" /></a>
  <a href="https://github.com/programmersd21/venti/issues"><img src="https://img.shields.io/github/issues/programmersd21/venti?style=for-the-badge&logo=github&logoColor=f38ba8&color=f38ba8&labelColor=313244" alt="issues" /></a>
  <a href="https://github.com/programmersd21/venti/stargazers"><img src="https://img.shields.io/github/stars/programmersd21/venti?style=for-the-badge&logo=github&logoColor=f5c2e7&color=f5c2e7&labelColor=313244" alt="stars" /></a>
  <a href="https://github.com/programmersd21/venti/commits/main"><img src="https://img.shields.io/github/last-commit/programmersd21/venti?style=for-the-badge&logo=git&logoColor=94e2d5&color=94e2d5&labelColor=313244" alt="last commit" /></a>
  <a href="https://github.com/programmersd21/venti"><img src="https://img.shields.io/github/repo-size/programmersd21/venti?style=for-the-badge&logo=github&logoColor=89dceb&color=89dceb&labelColor=313244" alt="repo size" /></a>
</p>

> [!NOTE]\
> terminal graphics support is currently experimental and may be unreliable. contributions and help with protocol compatibility, rendering bugs, and testing are very welcome. see [graphics protocol support](#graphics-protocol-support).

## demo

[![venti demo](https://img.youtube.com/vi/GsD61wQLam0/maxresdefault.jpg)](https://youtu.be/GsD61wQLam0)

## features

- **multiple workspaces**: organize panes across independent workspace sets
- **tiled layouts**: dwindle (default) or monocle, with configurable splits
- **floating panes**: drag-and-drop, drag-resize corners
- **four keybinding profiles**: beginner (mouse+f-keys), hypr (hjkl power set), vim (vi grammar), dev (editor/IDE ergonomics)
- **copy mode**: vim-like scrollback search and selection
- **live settings**: edit 40+ options via f2, changes apply instantly
- **bright ui**: centered dialogs, colorful CLI
- **graphics (experimental)**: sixel, kitty image protocol, iterm2 inline images - see [graphics protocol support](#graphics-protocol-support)
- **workspace tab context menu**: right-click a tab for rename / new / close, anchored at the click
- **scroll indicator**: optional "↑12" marker in the pane title while scrolled back
- **sidebar fold**: `shift+tab` folds the workspace sidebar away in every profile
- **fast**: event-driven, diff-based updates, minimal cpu

## install

```bash
git clone https://github.com/programmersd21/venti
cd venti
cargo build --release
sudo cp target/release/venti /usr/local/bin/
```

## quick start

```bash
venti
```

auto-creates config at `~/.config/venti/config.toml`.

- `f1` = help + keybindings
- `f2` = settings menu (live editor, including switching the four profiles)
- `f11` = command palette
- `f12` = theme picker

press `f1` for complete reference or read `docs/keybindings.md`.

## keybindings

### workspace navigation
```
alt+1..9     switch to workspace 1-9
alt+n        new workspace
alt+w        close workspace
```

### pane navigation (beginner, default profile)
```
alt+arrows    focus left/down/up/right
f10           resize mode
f3            new pane (split right)
f4            close pane
```

hypr/vim profiles bind `alt+hjkl` to focus and `alt+shift+hjkl` to swap; dev profile binds arrows + `alt+enter` new pane. see `docs/keybindings.md`.

### functions
```
f1            help
f2            settings
f5            zoom toggle (monocle)
f6            floating toggle
f8            search scrollback
f9            copy mode
f11           command palette
f12           theme menu
```

in hypr and vim profiles:
```
alt+space      command palette     (hypr)
alt+p          rotate split        (hypr)
alt+t          theme menu
alt+s          toggle sidebar
```

all multiplexer bindings use **alt** (escape-prefixed, reliable everywhere terminals speak). apps receive untouched plain ctrl+key sequences.

full reference: `docs/keybindings.md`

custom bindings: edit `[binds]` in `~/.config/venti/config.toml`

## configuration

`~/.config/venti/config.toml` (auto-created):

```toml
[general]
shell = ""   # empty: $SHELL, falls back to /bin/sh
scrollback = 10000
graphics = false  # experimental; set true to try images

[decorations]
border_style = "rounded"
pet = true
pet_emoji = "🐱"

[ui]
show_pwd = true
show_hostname = true
workspace_bar = "top"

[theme]
name = "mocha"  # 15 builtin dark themes

[binds]
# override any keybinding
"alt+3" = "workspace_goto 2"
"alt+shift+3" = "move_to_workspace 3"
"alt+shift+w" = "close_workspace"
```

live editing: press `f2` in venti to adjust 40+ settings.

docs: `docs/configuration.md`

## design

- **linux only**: vt100 terminal emulation via term-wm-vt100 (patched)
- **fast**: event-driven, minimal allocations, diff-based rendering
- **minimal deps**: ratatui, crossterm, serde, nix syscalls
- **no ui framework overhead**: custom rendering for perfect pixel control
- **bright & readable**: catppuccin mocha + 14 other dark themes
- **four profiles**: beginner, hypr, vim, dev, composable commands, fast fingers
- **alt-first keybindings**: escape-prefixed combos work in most terminals (see docs)

see `docs/architecture-assessment.md` for known limitations and planned
improvements (terminal emulator migration, per-pane cursor shape, graphics
ownership redesign).

## commands

```bash
venti                 # attach to default session
venti new -s myproject   # create and attach to a named session
venti ls            # list sessions
venti send -t default "ls -la"   # send keys to pane
venti run -t default workspace_goto 2  # run command
venti kill -t default # kill session
venti doctor          # system check
venti config check    # validate config
```

## development

```bash
cargo build           # debug build
cargo build --release # release binary
cargo test --no-fail-fast  # run all tests
cargo test --test keybindings_regression_test  # keybinding tests
cargo test --test graphics_conformance_test    # real kitten icat / chafa round-trip
cargo clippy          # lint (zero warnings)
```

## documentation

- `docs/quickstart.md` - hands-on tutorial
- `docs/keybindings.md` - complete reference
- `docs/configuration.md` - config file guide
- `docs/architecture.md` - internal design
- `docs/protocol.md` - wire protocol specification
- `docs/graphics-unicode-placeholders.md` - graphics protocols and unicode placeholders
- `docs/kitty-config.md` - kitty configuration for use with venti
- `docs/zsh.md` - zsh setup and completions
- `docs/index.md` - faq and troubleshooting

## graphics protocol support

venti is still working towards reliable terminal graphics protocol support.

image rendering is currently unreliable, and some protocols may not work as expected across terminal emulators, particularly during pane resizing, scrolling, and layout changes.

this is an area where help is genuinely welcome.

if you have experience with terminal graphics protocols, rendering pipelines, or terminal emulator compatibility, contributions, debugging help, and technical suggestions are very welcome.

if you encounter issues, have ideas, or know how to improve the implementation, please open an issue or start a discussion. even small reproductions and compatibility reports are valuable.

**i would especially appreciate help from people familiar with:**

- kitty graphics protocol
- sixel
- iterm2 image protocol
- terminal rendering and multiplexer compatibility

venti is an independent project, and improving graphics support is an ongoing effort. thanks to everyone willing to investigate, test, and contribute.

graphics is off by default. it is experimental; set `graphics = true` under
`[general]` to try it. `venti doctor` prints the same reminder.

**where to help:**

- **github issues**: concrete bugs, reproducible failures, compatibility reports
- **github discussions**: technical ideas, implementation approaches, requests for help
- **pull requests**: actual fixes and improvements

**current state, honestly:**

| protocol | tested | state |
|---|---|---|
| kitty graphics | **most tested and most stable** | transmit, place, re-place, crop-to-pane, scrollback anchoring, unicode placeholders, file-backed (`t=f`/`t=s`) transmissions (read once, re-sent inline and chunked) all work; edge cases remain around float/reflow geometry |
| sixel | weaker | parse, render and scroll work; needs a sixel-capable outer terminal; re-placement after a resize rides on a full `ESC[2J` repaint |
| iterm2 inline images | not well tested | display works; no cropping, no source-offset re-placement, no per-pane clipping |

kitty is the protocol to rely on today: it is what `kitten icat`, yazi and fastfetch use, and every code path is covered by unit tests plus the real-tool conformance suite. sixel is a fallback for terminals without kitty support. iterm2 is best-effort.

images are re-transmitted in spec-compliant chunks (max 4096 base64 bytes per apc, `m=1`/`m=0`), the payload never leaks into the text stream, and file-backed pixels are cached at transmission time so the app deleting its temp file cannot blank the frame.

the parsers and the framing are covered by `cargo test graphics`, and `tests/graphics_conformance_test.rs` runs the real tools (`kitten icat`, `chafa -f sixels`) against a real server to check that the payload never reaches the text stream, that the image actually arrives and that every chunk respects the 4096-byte limit.

## contributing

issues, discussions and pull requests are welcome - see `contributing.md` for the workflow, the code style and how to run the test suite.

graphics protocol work is the area where help is most needed, see [graphics protocol support](#graphics-protocol-support).

## license

mit

## credits

inspired by tmux, zellij, helix.

built with ratatui, crossterm, serde.

colors from catppuccin.

---

**venti 0.1.0**, 2026-11-16

*a fast, quiet terminal multiplexer for linux.*
