# configuration

venti stores its configuration in `~/.config/venti/config.toml`. settings can be modified by directly editing this file or by pressing `f2` inside venti to use the live settings menu.

## example config

```toml
[general]
shell = ""               # defaults to $SHELL, falls back to /bin/sh
scrollback = 10000       # scrollback line buffer per pane
graphics = true          # enable kitty and sixel graphics forwarding
profile = "beginner"     # "beginner", "hypr", "vim", or "dev"

[decorations]
border_style = "rounded" # "none", "plain", "rounded", "thick", "double"
gap_inner = 0            # spacing between panes in cells
gap_outer = 0            # spacing around terminal window edge
pet = true               # enable companion mascot
pet_emoji = "🐱"         # mascot icon

[ui]
show_pwd = true          # show current working directory in pane header
show_hostname = true     # show system hostname in status bar
workspace_bar = "top"    # "top", "bottom", or "none"
dim_inactive = true      # dim unfocused panes
dim_strength = 0.35      # dim factor between 0.0 and 1.0

[theme]
name = "mocha"           # builtin theme name (e.g. mocha, macchiato, frappe, latte, nord)

[binds]
# custom bindings override defaults
"alt+d" = "split_horizontal"
"alt+e" = "split_vertical"
```

## live settings menu (`f2`)

pressing `f2` opens the live menu overlay. any changes made in this menu are written directly to `config.toml` and applied across all active workspaces and panes in real time without restarting the daemon.

## validating configuration

verify your configuration syntax and options using the cli:
```bash
venti config check
```
