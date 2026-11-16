# quickstart

venti 0.1.0 | 2026-11-16 | linux only

## install & run

```bash
cargo build --release
./target/release/venti
```

config auto-created at `~/.config/venti/config.toml` on first run.

## basic workflow

### rename a workspace

right-click the workspace tab in the tab bar (or in the sidebar) and pick a
menu entry: `n` new, `r` rename, `c` close.

### switch workspaces (alt + digit)
- `alt+1` = workspace 1
- `alt+2` = workspace 2
- `alt+3` = workspace 3
- (all 9 workspaces available: `alt+1`..`alt+9`)

### navigate panes (beginner profile)
- `alt+arrows` = focus left/down/up/right
- hypr/vim: `alt+hjkl` focus, `alt+shift+hjkl` swap. dev: `alt+arrows` focus

### create/close
- `f3` = new pane (split right)
- `f4` = close pane
- `alt+n` = new workspace
- `alt+w` = close workspace
- `alt+shift+q` = kill session

### move panes (alt+shift + digit/hjkl)
- `alt+shift+1` = move pane to workspace 1
- `alt+shift+h` = swap pane left
- `alt+shift+j` = swap pane down

## keybinding profiles

venti ships four profiles, switch anytime with `f2` (arrow to profile, enter to
cycle, or set `[profile] mode` in config.toml):

- **beginner** (default), mouse + f-keys, `alt+arrows` focus panes
- **hypr**, hjkl focus/swap, alt+arrows workspace cycle
- **vim**, vi grammar: `alt+[`/`alt+]` workspaces, `alt+-`/`alt+=` resize,
  `alt+x` close, `alt+p` palette
- **dev**, editor/IDE flow: alt+arrows focus, `alt+enter` new pane, `alt+k` palette

## core functions (f-keys)

- `f1` = help
- `f2` = settings
- `f5` = zoom (one pane fills the workspace)
- `f6` = float or unfloat the pane
- `f8` = search scrollback
- `f9` = copy mode
- `f10` = resize mode (arrows)
- `f11` = command palette
- `f12` = theme picker
- `shift+tab` = fold the workspace sidebar away

## common tasks

### split current pane

```
f3            # split right
f10           # resize mode (h/j/k/l)
alt+shift+j   # swap panes
```

### move pane to different workspace

```
alt+shift+2   # move to workspace 2 (while pane focused)
```

### copy from scrollback

```
f8            # search mode
<type text>   # search (case-insensitive substring)
enter         # next match (shift+enter prev)
f9            # copy mode
v             # select
y             # copy
```

### zoom pane

```
f5            # toggle zoom (full pane view)
f5            # toggle back
```

### adjust settings

```
f2            # open settings
up/down       # navigate
space         # toggle / cycle
enter         # edit value
alt+r         # reset to default
tab           # switch category
```

changes are saved to `~/.config/venti/config.toml` and the ones that can apply
while the session runs do so immediately; shell, scrollback and the pane and
workspace limits apply to new panes.

## customization

edit `~/.config/venti/config.toml`:

```toml
[general]
shell = "/bin/bash"
scrollback = 20000

[decorations]
border_style = "thick"
pet = true

[binds]
# override any keybinding
"alt+e" = "toggle_float"
"alt+w" = "workspace_next"
```

graphics is on by default. it is experimental: if images glitch on your terminal, turn it off with `graphics = false` under `[general]` (then `venti doctor` stops reminding you).

validate with `venti config check`.

## tips & tricks

**vim users:**
- `alt+hjkl` focus (hypr/vim), `alt+shift+hjkl` swap (hypr/vim)
- `f9` = copy mode (vim keys: hjkl, v, y)
- `alt+left/right` workspace cycle (hypr), `alt+[`/`]` (vim/dev)

**multiple sessions:**
```bash
# separate session per project
venti new -s project1
venti new -s project2

# list sessions
venti ls
```

**scripting:**
```bash
# send command to session
venti send -t default "ls -la"

# attach to specific session
venti attach -t default

# run a command against the session
venti run -t default workspace_goto 2

# kill session
venti kill -t default
```

**pass keys to application:**
if your app needs a key venti uses, configure passthrough in `~/.config/venti/config.toml`:

```toml
[input]
passthrough = ["alt+h", "alt+w", "alt+n"]
```

## keyboard help

all bindings visible:
- press `f1` in venti = interactive help with scrolling
- read `docs/keybindings.md` = complete reference
- run `venti --help` = cli options

## first night survival

1. **open venti**: `venti`
2. **read welcome**: extended tutorial shows on startup
3. **press f1**: full keybindings reference
4. **press f2**: live settings editor (try themes!)
5. **press f11**: command palette (list all actions)
6. **press alt+2**: switch to workspace 2 and try it out
7. **press f3**: split your pane
8. **press f5**: zoom to full pane
9. **press f9**: enter copy mode, select text, press y to copy

enjoy!
