# configuration

venti 0.1.0 | 2026-11-16 | linux only

edit `~/.config/venti/config.toml` (created on first run with defaults).

## general

```toml
[general]
shell = "/bin/zsh"                    # default shell for panes
scrollback = 10000                    # lines to keep
osc52 = true                          # allow osc 52 clipboard
graphics = false                      # experimental; set true to try images
confirm_close_running = true          # ask before close
max_clients = 32                      # max simultaneous connections
max_panes = 1000                      # max per session
max_workspaces = 100                  # max per session
default_workspaces = 1                # create n on startup
workspace_names = []                  # custom names or empty
```

## input

```toml
[input]
mouse = true                          # enable mouse
copy_on_select = true                 # auto-copy selection
passthrough = ["alt+h", "alt+w"]     # forward to app
scroll_lines = 3                      # wheel delta
disabled_keys = []                    # disable: ["f2", "alt+n"]
```

## profile

```toml
[profile]
mode = "beginner"                     # "beginner" (default), "hypr", "vim", "dev"
```

four profiles ship in 0.1.0:

- **beginner** - mouse + f-keys + alt+arrows. the smallest chord map to memorize (default).
- **hypr** - hyprland-style hjkl focus/swap, alt+arrows workspace cycle.
- **vim** - vi grammar: hjkl only, alt+[ / alt+] workspace motion, -/= resize,
  alt+x close, alt+p palette, alt+e theme.
- **dev** - editor/IDE ergonomics: alt+arrows focus, alt+enter new pane,
  alt+k command palette, alt+[ / alt+] workspace cycle.

## layout

```toml
[layout]
mode = "dwindle"                      # "dwindle" or "monocle"
split_axis = "right"                  # "right", "down", or "auto"
gap_inner = 0                         # gap between panes
gap_outer = 1                         # gap at edges
resize_step = 5                       # cells per step
min_pane_cols = 6                     # minimum width (>= 6)
min_pane_rows = 2                     # minimum height
```

## decorations

```toml
[decorations]
border_style = "rounded"              # "plain", "rounded", "thick", "double", "none"
hide_border_when_single = false       # hide if only one pane
dim_inactive = true                   # dim non-focused
dim_strength = 0.18                   # dim amount (0.0-1.0)
title = true                          # show titles
pet = true                            # enable emoji
pet_emoji = "🐱"                       # emoji character
scroll_indicator = false              # show "↑12" in the title while scrolled back
```

## animation

```toml
[animation]
enabled = true                        # enable animations
layout_duration_ms = 320              # duration (ms)
layout_easing = "ease_out_cubic"      # "linear", "ease_out_cubic", "ease_in_out", "ease_out_quint"
```

## ui

```toml
[ui]
workspace_bar = "top"                 # "top", "bottom", "left", "right", or "off"
hint_bar = true                       # show hint bar
clock = true                          # show clock
clock_format = "%H:%M"                # time format
ascii = false                         # restrict glyphs to 7-bit ascii
show_tab_bar = true                   # show workspace tabs
show_hostname = true                  # show hostname in pane titles
show_time_in_title = false            # show time in pane titles
show_pwd = true                       # show working directory in pane titles
show_welcome = true                   # show the welcome overlay on start
frame_interval_ms = 8                 # minimum render interval
sidebar_width = 28                    # sidebar width (left/right bar mode)
sidebar_collapsed_width = 4           # sidebar width when collapsed
sidebar_header = true                 # show session name header
sidebar_collapsed = false             # start collapsed
sidebar_density = "comfortable"       # "compact" or "comfortable"
```

**clock_format** supported escapes:
- `%H` `%I` - hour, 24h / 12h (zero-padded)
- `%M` `%S` - minute, second (zero-padded)
- `%p` - am/pm
- `%R` `%T` - `%H:%M` / `%H:%M:%S`
- `%Y` `%m` `%d` `%e` - year, month, day (`%e` space-padded)
- `%A` `%a` - weekday, full / abbreviated
- `%B` `%b` - month name, full / abbreviated
- `%%` - literal percent

## theme

```toml
[theme]
name = "mocha"                        # built-in theme name
# optional color overrides on top of the named theme
# base = "#1e1e2e"
# accent = "#89b4fa"
```

every color slot of the named theme can be overridden (`base`, `mantle`,
`surface`, `overlay`, `text`, `subtext`, `muted`, `accent`, `info`, `success`,
`warn`, `alert`). an override that is not a valid hex color is ignored with a
warning and the built-in color stays.

**built-in themes:**
mocha, tokyo-night, gruvbox-dark, dracula, nord, catppuccin-frappe,
catppuccin-macchiato, everforest, kanagawa, rose-pine, rose-pine-moon,
solarized-dark, monokai, one-dark, vesper

use `f12` in venti to preview interactively.

## reload

venti watches the config file and reloads it live; parse errors keep the old
config and show a toast. ui and theme changes apply immediately;
session-level settings (shell, scrollback, max_panes, ...) apply to new panes
and on restart.

## live settings editor

press `f2` to open the settings menu (live config editor):
- `up/down` - navigate settings
- `space` - toggle booleans or cycle enums
- `enter` - edit strings/numbers
- `alt+r` - reset setting to default
- `tab` / `shift+tab` - switch category tabs
- `alt+1-4` - jump to quick/visuals/layout/advanced tab
- type to filter settings by name
- `esc` - close

changes apply instantly and save to `~/.config/venti/config.toml`.

note: settings changed through `f2` are written back preserving comments and
key order where possible, only changed keys are edited in place. if a change
cannot be expressed as line edits, the file is rewritten wholesale and
comments are lost. saving with no changes leaves the file untouched.

## customization

override or add keybindings with `[binds]` in config.toml:

```toml
[binds]
"alt+3" = "workspace_goto 2"
"alt+shift+3" = "move_to_workspace 3"
"alt+shift+w" = "close_workspace"              # close current workspace
"alt+shift+q" = "close_workspace 1 2 3"        # close multiple workspaces
"alt+e" = "command_palette"
```

run `venti config check` to validate. unknown keys/commands rejected.

## examples

**vim setup:**
```toml
[input]
passthrough = ["alt+h", "alt+w", "alt+n", "alt+a"]

[layout]
split_axis = "right"
```

**dark minimal:**
```toml
[theme]
name = "dracula"

[ui]
sidebar_density = "compact"

[decorations]
pet = false
```

**performance (no animations):**
```toml
[animation]
enabled = false

[theme]
name = "nord"

[ui]
sidebar_width = 20
```

## environment

respected:
- `$SHELL` - fallback shell when `general.shell` is unset
- `$EDITOR` - editor for config
- `$XDG_CONFIG_HOME` - config dir (default: ~/.config)
- `$XDG_STATE_HOME` - state/log dir (default: ~/.local/state)
- `$XDG_RUNTIME_DIR` - socket dir (default: /run/user/uid)

## security

never put passwords in config. use env vars or pass at runtime:
```sh
db_pass=mypass venti run "search users"
```
