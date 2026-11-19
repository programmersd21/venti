<p align="center">
  <img src="assets/logo.png" alt="venti logo" />
  <br>
  fast, quiet terminal multiplexer for linux.
  <br><br>
  <a href="https://github.com/programmersd21/venti/actions/workflows/ci.yml"><img src="https://img.shields.io/github/actions/workflow/status/programmersd21/venti/ci.yml?style=for-the-badge&logo=githubactions&logoColor=a6e3a1&label=build&color=a6e3a1&labelColor=313244" alt="build" /></a>
  <a href="https://crates.io/crates/venti"><img src="https://img.shields.io/crates/v/venti?style=for-the-badge&logo=rust&logoColor=cba6f7&color=cba6f7&labelColor=313244" alt="crates.io" /></a>
  <a href="https://aur.archlinux.org/packages/venti-bin"><img src="https://img.shields.io/aur/version/venti-bin?style=for-the-badge&logo=archlinux&logoColor=89b4fa&color=89b4fa&labelColor=313244" alt="AUR version" /></a>
  <a href="https://github.com/programmersd21/venti/blob/main/license"><img src="https://img.shields.io/github/license/programmersd21/venti?style=for-the-badge&logo=opensourceinitiative&logoColor=f9e2af&color=f9e2af&labelColor=313244" alt="license" /></a>
  <a href="https://www.rust-lang.org"><img src="https://img.shields.io/badge/rust-2024-fab387?style=for-the-badge&logo=rust&logoColor=fab387&labelColor=313244" alt="rust" /></a>
  <a href="https://github.com/programmersd21/venti/issues"><img src="https://img.shields.io/github/issues/programmersd21/venti?style=for-the-badge&logo=github&logoColor=f38ba8&color=f38ba8&labelColor=313244" alt="issues" /></a>
  <a href="https://github.com/programmersd21/venti/stargazers"><img src="https://img.shields.io/github/stars/programmersd21/venti?style=for-the-badge&logo=github&logoColor=f5c2e7&color=f5c2e7&labelColor=313244" alt="stars" /></a>
  <a href="https://github.com/programmersd21/venti/commits/main"><img src="https://img.shields.io/github/last-commit/programmersd21/venti?style=for-the-badge&logo=git&logoColor=94e2d5&color=94e2d5&labelColor=313244" alt="last commit" /></a>
  <a href="https://github.com/programmersd21/venti"><img src="https://img.shields.io/github/repo-size/programmersd21/venti?style=for-the-badge&logo=github&logoColor=89dceb&color=89dceb&labelColor=313244" alt="repo size" /></a>
</p>

## demo

[![venti demo](https://img.youtube.com/vi/GsD61wQLam0/maxresdefault.jpg)](https://youtu.be/GsD61wQLam0)

## features

- **Sixel graphics**: clipped and tracked with pane scrolling, powered by `icy_sixel`
- **Kitty graphics protocol**: image transmission, placements, scrolling, and resize support
- **multiple workspaces**: organize panes across independent workspace sets
- **tiled layouts**: dwindle (default) or monocle, with configurable splits
- **floating panes**: drag-and-drop, drag-resize corners
- **four keybinding profiles**: beginner (mouse+f-keys), hypr (hjkl power set), vim (vi grammar), dev (editor/IDE ergonomics)
- **copy mode**: vim-like scrollback search and selection
- **live settings**: edit 40+ options via f2, changes apply instantly
- **bright ui**: centered dialogs, colorful CLI
- **workspace tab context menu**: right-click a tab for rename / new / close, anchored at the click
- **scroll indicator**: optional "↑12" marker in the pane title while scrolled back
- **sidebar fold**: `shift+tab` folds the workspace sidebar away in every profile
- **fast**: event-driven, diff-based updates, minimal cpu

## install

### arch Linux

```bash
yay -S venti-bin
```

use `paru -S venti-bin` if you use **paru**.

### build from source

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
graphics = true  # enables Kitty and Sixel graphics forwarding

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
- **graphics support**: Kitty graphics and Sixel are selected from the attached terminal's `TERM` and `TERM_PROGRAM` identity
- **fast**: event-driven, minimal allocations, diff-based rendering
- **minimal deps**: ratatui, crossterm, serde, nix syscalls
- **no ui framework overhead**: custom rendering for perfect pixel control
- **bright & readable**: catppuccin mocha + 14 other dark themes
- **four profiles**: beginner, hypr, vim, dev, composable commands, fast fingers
- **alt-first keybindings**: escape-prefixed combos work in most terminals (see docs)

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
cargo test --test sixel_graphics_test  # sixel rendering verification
cargo clippy          # lint
```

## documentation

- `docs/quickstart.md` - hands-on tutorial
- `docs/keybindings.md` - complete reference
- `docs/configuration.md` - config file guide
- `docs/architecture.md` - internal design
- `docs/protocol.md` - wire protocol specification
- `docs/zsh.md` - zsh setup and completions
- `docs/index.md` - faq and troubleshooting

## graphics

Venti forwards Kitty graphics protocol and Sixel sequences, subject to the attached terminal's detected capabilities. iTerm2 image sequences are unsupported. See [graphics documentation](docs/graphics.md) for protocol scope and limitations.

## contributing

issues, discussions and pull requests are welcome - see `contributing.md` for the workflow, the code style and how to run the test suite.

## license

mit

## credits

inspired by tmux, zellij, helix.

built with ratatui, crossterm, serde, icy_sixel, ratatui-image.

colors from catppuccin.

---

**venti 0.2.0**, 2026-11-19

*a fast, quiet terminal multiplexer for linux.*
