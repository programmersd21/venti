# keybindings

venti ships with four distinct keybinding profiles. all profiles use `alt` chords or function keys to avoid capturing `ctrl` sequences used by shells, text editors, and interactive terminal tools.

## profiles overview

switch profiles dynamically inside venti by pressing `f2` -> `general` -> `profile`, or set `general.profile` in `~/.config/venti/config.toml`.

- **beginner (default)**: arrow keys for pane focus, function keys for pane lifecycle.
- **hypr**: wm-style `alt+hjkl` navigation, `alt+space` command palette.
- **vim**: vi-grammar navigation and pane management.
- **dev**: editor-focused navigation with `alt+enter` splits.

---

## universal shortcuts (all profiles)

### workspaces
| shortcut | action |
|---|---|
| `alt+1` .. `alt+9` | switch to workspace 1 through 9 |
| `alt+n` | create a new workspace |
| `alt+w` | close active workspace |
| `alt+left` / `alt+right` | previous / next workspace |
| `alt+shift+1` .. `9` | move active pane to workspace 1..9 |
| `shift+tab` | fold / unfold sidebar |

### functional overlays
| shortcut | action |
|---|---|
| `f1` | help overlay and keybindings list |
| `f2` | live settings menu |
| `f5` | toggle zoom (monocle layout) |
| `f6` | toggle floating pane mode |
| `f8` | search scrollback buffer |
| `f9` | scrollback copy mode |
| `f11` | command palette |
| `f12` | theme switcher |
| `alt+q` | close active pane or modal |

---

## beginner profile (default)

| shortcut | action |
|---|---|
| `alt+left` / `down` / `up` / `right` | focus left / down / up / right |
| `f3` | split pane horizontally |
| `f4` | close active pane |
| `f10` | enter resize mode (arrows resize, esc exits) |

---

## hypr profile

| shortcut | action |
|---|---|
| `alt+h` / `j` / `k` / `l` | focus left / down / up / right |
| `alt+shift+h` / `j` / `k` / `l` | swap pane left / down / up / right |
| `alt+v` | split vertical (side by side) |
| `alt+b` | split horizontal (stacked) |
| `alt+p` | rotate split direction |
| `alt+space` | command palette |
| `alt+s` | toggle sidebar |
| `alt+t` | theme menu |

---

## vim profile

| shortcut | action |
|---|---|
| `alt+h` / `j` / `k` / `l` | focus left / down / up / right |
| `alt+s` | horizontal split |
| `alt+v` | vertical split |
| `alt+c` | close active pane |
| `alt+t` | theme menu |

---

## dev profile

| shortcut | action |
|---|---|
| `alt+left` / `down` / `up` / `right` | focus left / down / up / right |
| `alt+enter` | split pane right |
| `alt+shift+w` | close active pane |
| `alt+p` | command palette |

---

## custom keybindings

remap or bind any action in `~/.config/venti/config.toml` under `[binds]`:

```toml
[binds]
"alt+d" = "split_horizontal"
"alt+e" = "split_vertical"
"alt+z" = "toggle_zoom"
```
