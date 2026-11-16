# keybindings

venti 0.1.0 | 2026-11-16 | linux only

multiplexer bindings use **alt** everywhere. plain ctrl chords are unbound by
venti; apps receive the raw C0 bytes untouched. f-keys always work.

## workspace navigation & management

### quick switch (alt + digit)
digits are reliable across every terminal (alt arrives as esc + digit):

| key | action |
|-----|--------|
| `alt+1` | switch to workspace 1 |
| `alt+2` | switch to workspace 2 |
| `alt+3` | switch to workspace 3 |
| `alt+4` | switch to workspace 4 |
| `alt+5` | switch to workspace 5 |
| `alt+6` | switch to workspace 6 |
| `alt+7` | switch to workspace 7 |
| `alt+8` | switch to workspace 8 |
| `alt+9` | switch to workspace 9 |

### move pane to workspace (alt+shift + digit)

| key | action |
|-----|--------|
| `alt+shift+1` | move pane to workspace 1 |
| `alt+shift+2` | move pane to workspace 2 |
| `alt+shift+3` | move pane to workspace 3 |
| `alt+shift+4` | move pane to workspace 4 |
| `alt+shift+5` | move pane to workspace 5 |
| `alt+shift+6` | move pane to workspace 6 |
| `alt+shift+7` | move pane to workspace 7 |
| `alt+shift+8` | move pane to workspace 8 |
| `alt+shift+9` | move pane to workspace 9 |

### navigate sequentially

| key | action |
|-----|--------|
| `alt+right` / `alt+left` | next / previous workspace |
| `alt+shift+right` / `alt+shift+left` | move pane to next / previous workspace |
| `alt+n` | new workspace |
| `alt+w` | close workspace |

## pane navigation & management

### vim-style navigation (alt + hjkl)

| key | action |
|-----|--------|
| `alt+h` | focus left |
| `alt+j` | focus down |
| `alt+k` | focus up |
| `alt+l` | focus right |

### vim-style swap (alt+shift + hjkl)

swap panes with neighbors:

| key | action |
|-----|--------|
| `alt+shift+h` | swap left |
| `alt+shift+j` | swap down |
| `alt+shift+k` | swap up |
| `alt+shift+l` | swap right |

### resize pane (f10 or mouse border-drag)

direct `alt+shift+arrows` resize was removed, use f10 resize mode, or drag a border with the mouse.

| key | action |
|-----|--------|
| `f10` | resize mode (h/j/k/l or arrows, enter/esc to exit) |

### pane actions

| key | action |
|-----|--------|
| `alt+q` | close focused pane |
| `f3` | new pane (split right) |
| `f4` | close pane |

## special functions

| key | action |
|-----|--------|
| `alt+space` | command palette (alias of `f11`) |
| `alt+t` | theme menu (alias of `f12`) |
| `alt+s` | toggle sidebar |
| `alt+p` | rotate split |

## closing & exiting

| key | action |
|-----|--------|
| `alt+shift+q` | exit venti (kill session) |

## function keys (always active)

f-keys work everywhere without modifiers:

| key | action |
|-----|--------|
| `f1` | help / keybindings |
| `f2` | settings menu |
| `f5` | zoom toggle (pane fills the workspace) |
| `f6` | floating toggle |
| `f7` | scratchpad toggle |
| `f8` | search scrollback |
| `f9` | copy mode |
| `f10` | resize mode |
| `f11` | command palette |
| `f12` | theme picker |

## colon commands

type `:` to enter vim-style command mode. unknown commands are rejected.
arguments work as expected (`workspace_goto 3`, `move_to_workspace 2 4`,
`close_workspace 3`):

| command | alias | action |
|---------|-------|--------|
| `new_pane` | - | new pane (split right) |
| `split_right` | - | split right |
| `split_down` | `split`, `sp` | split down |
| `close_pane` | `close` | close focused pane |
| `focus_left` / `focus_down` / `focus_up` / `focus_right` | - | move focus |
| `focus_next` / `focus_prev` | - | cycle focus |
| `swap_left` / `swap_down` / `swap_up` / `swap_right` | - | swap pane with neighbor |
| `resize_left` / `resize_down` / `resize_up` / `resize_right` | - | resize focused pane |
| `resize_mode` | - | toggle resize mode |
| `equalize` | - | equalize splits |
| `rotate_split` | - | rotate split axis |
| `toggle_zoom` | `only` | zoom toggle |
| `toggle_sidebar` | - | toggle sidebar |
| `toggle_float` | - | floating toggle |
| `float_center` | - | center floating pane |
| `workspace_goto n [n...]` | `buffer n`, `b n`, `tabnext n`, `tabn n` | switch to workspace(s) |
| `workspace_next` | `tabnext`, `tabn` | next workspace |
| `workspace_prev` | `tabprev`, `tabp` | previous workspace |
| `new_workspace` | `tabnew` | new workspace |
| `close_workspace [n...]` | `tabclose`, `tabc` | close current (or given) workspace(s) |
| `move_to_workspace n [n...]` | - | move pane to workspace(s) |
| `move_to_workspace_next` / `move_to_workspace_prev` | - | move pane to next / prev workspace |
| `rename_workspace <name>` | - | rename current workspace |
| `toggle_scratch` | - | scratchpad toggle |
| `copy_mode` | - | copy mode |
| `search` | - | search scrollback |
| `paste_buffer` | `w`, `write` | paste clipboard into focused pane |
| `command_palette` | - | command palette |
| `theme_menu` | - | theme picker |
| `settings_menu` | `settings` | settings menu |
| `help` | - | help screen |
| `reload_config` | - | reload config file |
| `detach` | - | detach from session |
| `kill_session` | `quit`, `q`, `qa`, `q!`, `qa!` | kill session |
| `show_welcome` | - | show welcome overlay |
| `cat` | - | ascii cat visits your pane |

## copy mode (f9)

| key | action |
|-----|--------|
| `h/j/k/l` or arrows | move cursor |
| `v` | start/end character selection |
| `y` | yank selection to clipboard |
| `esc` or `q` | exit |

## search mode (f8)

| key | action |
|-----|--------|
| text | filter (case-insensitive substring) |
| `enter` | next match |
| `shift+enter` | previous match |
| `backspace` | remove character |
| `esc` | exit |

## settings menu (f2)

| key | action |
|-----|--------|
| `up/down` | navigate |
| `tab` / `shift+tab` | switch category |
| `space` | toggle boolean / cycle enum |
| `enter` | edit value |
| `alt+r` | reset setting to default |
| `alt+1-4` | jump to tab |
| `tab` / `shift+tab` | switch category |
| click a row | select it |
| wheel | scroll the list |
| `esc` | close |

**edit mode:**
- `left/right` ±1, `shift+left/right` ±10
- type to replace, `backspace` to delete
- `enter` save, `esc` cancel

## palette (f11)

| key | action |
|-----|--------|
| text | filter commands (fuzzy) |
| `up/down` | navigate |
| `enter` | execute typed text if it parses, otherwise the selection |
| `esc` | cancel |

arguments work directly: typing `workspace_goto 3` then `enter` switches to workspace 3.
commands that take arguments accept them: `workspace_goto 1 2`, `move_to_workspace 3`,
`rename_workspace my ws`, `close_workspace 2`.
`alt+1..9` binds workspaces 1-9; the palette and `:` line accept any number and create the workspace on the fly.

## theme picker (f12)

| key | action |
|-----|--------|
| `up/down` | navigate (preview) |
| `enter` | apply |
| `esc` | cancel |

## mouse

| action | effect |
|--------|--------|
| left click pane | focus |
| right click workspace tab | tab context menu (new / rename / close workspace) |
| right click anywhere else | no-op |
| wheel | scroll |
| drag border | resize split |
| drag float title | move |
| `alt+drag` | swap panes |

the tab menu: `up/down` or the wheel selects, `enter` runs, `esc` or a click
outside dismisses, and the `n`/`r`/`c` accelerators jump straight to an entry.

`shift+tab` folds the workspace sidebar away in every profile.

## resize mode (f10)

| key | action |
|-----|--------|
| `h/j/k/l` or arrows | resize left/down/up/right |
| `enter` / `esc` | exit |

## profile modes

`f2` → profile switches live; every profile always honors the f-keys above.

### beginner (default), mouse + arrows
keyboard-driven mouse users and people learning the surface.
- `alt+1..9` goto workspace 1-9
- `alt+shift+1..9` move pane to workspace 1-9
- `alt+n` new workspace · `alt+w` close workspace
- `alt+arrow` focus pane (left/down/up/right, no hjkl to memorize)
- `alt+q` close pane

### hypr, hjkl tiling power set
full keyboard flow, hyprland-flavored density.
- `alt+h/j/k/l` focus left/down/up/right · `alt+shift+h/j/k/l` swap
- `alt+left/right` prev/next workspace · `alt+shift+left/right` move pane
- `alt+,` / `alt+.` workspace prev/next (alias) · `alt+shift+,` / `alt+shift+.` move pane (alias)
- `alt+q` close pane · `alt+shift+q` kill session
- `alt+n` new workspace · `alt+w` close workspace
- `alt+space` command palette · `alt+t` theme menu · `alt+s` sidebar · `alt+p` rotate split

### vim, vi grammar
the vi grammar, for people who live in the editor.
- `alt+h/j/k/l` focus · `alt+shift+h/j/k/l` swap
- `alt+[` / `alt+]` workspace prev/next
- `alt+-` / `alt+=` resize split width
- `alt+x` close pane · `alt+shift+x` kill session
- `alt+z` toggle zoom · `alt+e` theme menu · `alt+p` command palette
- `alt+n` new workspace · `alt+w` close workspace · `alt+s` sidebar
- `alt+shift+left/right` move pane to prev/next workspace

### dev, editor/IDE ergonomics
the muscle memory of an editor or an ide.
- `alt+arrow` focus left/down/up/right
- `alt+enter` new pane · `alt+q` close pane
- `alt+[` / `alt+]` workspace prev/next
- `alt+k` command palette · `alt+t` theme menu
- `alt+n` new workspace · `alt+w` close workspace
- `alt+shift+left/right` move pane to prev/next workspace

## design principles

**alt-first:** all multiplexer bindings live on the alt key, because escape
prefixed combos (`esc` + byte) are delivered identically on every terminal.
applications receive plain ctrl+key sequences.

**why not ctrl?**
- `ctrl+1..9` and `ctrl+letter` clash with shell line-editing (`ctrl+a`/`ctrl+e`),
  `ctrl+w` (kill word), `ctrl+l` (clear), etc.
- kitty protocol reports them fine, but legacy terminals can't distinguish
  `ctrl+1` from `ctrl+a` (both arrive as byte `0x01`)
- `alt+` chord sequences never collide with shell editing (alt is a dead
  modifier for bourne-style line editors that read raw bytes)

**f-keys:** always work as fallback for all critical functions.

## customization

override or add bindings with `[binds]` in `~/.config/venti/config.toml`:

```toml
[binds]
"alt+3" = "workspace_goto 2"
"alt+e" = "command_palette"
"alt+w" = "workspace_next"
```

run `venti config check` to validate syntax. unknown keys/commands are rejected.

## notes

**apps get plain ctrl:**
- `ctrl+a` is byte `0x01`; every app receives it unchanged
- `ctrl+c` is byte `0x03`; kill your cat with it, same as anywhere
- `ctrl+1..9` under the kitty protocol report as csi `49;5u` etc.; apps see
  them verbatim too

**alt is empty in shells:**
- `alt+h/j/k/l` and `alt+1..9` do nothing in zsh/bash by default. if you bind
  them there yourself (e.g. `bindkey "^[h" backward-kill-word`), the shell wins
  for focused panes (pass it through or disable that binding in `[binds]`).

**alt+tab is not bound at all**, window managers grab it globally, so venti
would never receive it. in hypr use `alt+right` / `alt+left`; in vim/dev use `alt+[` / `alt+]`.

**how alt+shift arrives on the wire:**
- plain alt+letter: `esc` + lowercase byte (e.g. `esc h` = `alt+h`)
- alt+shift+letter: `esc` + uppercase byte (e.g. `esc H` = `alt+shift+h`)
- alt+shift+digit: `esc` + shifted glyph (e.g. `esc !` = `alt+shift+1`)
- alt+shift+punct: same trick, `esc <` = `alt+shift+,`, `esc >` = `alt+shift+.`,
  `esc {`, etc. venti normalizes these glyphs back to base+shift
- xterm/kitty modifier form also works: `csi 1;4A` = `alt+shift+up`

**alt+chords** work across terminals that deliver escape-prefixed bytes natively
(kitty, foot, wezterm, xterm); a few override them, see `docs/kitty-config.md`.

## examples

**quick setup:**
```bash
# just start using it - defaults are sensible
venti

# press f1 for help, f2 for settings, f11 for command palette
```

**customize for emacs users:**
```toml
[binds]
"alt+x" = "command_palette"
"alt+o" = "workspace_next"
```

**customize for vim users:**
```toml
[binds]
"alt+l" = "focus_right"
"alt+w" = "workspace_next"
"alt+e" = "toggle_float"
```
