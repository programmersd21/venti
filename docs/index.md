# documentation

venti 0.1.0 | 2026-11-16 | linux only

## start here

- **[quickstart.md](quickstart.md)** - install, first session, basic usage
- **[keybindings.md](keybindings.md)** - full keybinding reference
- **[configuration.md](configuration.md)** - config file reference

## technical

- **[architecture.md](architecture.md)** - system design, internals
- **[protocol.md](protocol.md)** - wire protocol specification
- **[graphics-unicode-placeholders.md](graphics-unicode-placeholders.md)** - kitty graphics, sixel and iterm2 behaviour

## faq

**what's different from tmux/screen?**

venti is simpler: fewer features, faster startup, modern terminal support (kitty keys, proper mouse, graphics). not designed for tmux/screen compatibility.

**does it work with kitty/alacritty/iterm2?**

yes. terminal graphics are experimental and their behaviour depends on the
outer terminal: kitty and iterm2 inline images work best, other terminals get
sixel. run `venti doctor` to check what your terminal negotiates.

a sixel image only renders when the outer terminal itself speaks sixel
(xterm, foot, wezterm, konsole). kitty does not do sixel at all: inside kitty,
use the kitty protocol instead, e.g. `chafa -f kitty img.png` or `kitten icat`.

**can i use vim/emacs keybindings?**

yes. switch `[profile] mode` in config.toml to one of:
`beginner` (default, mouse + arrows) · `hypr` (hjkl) · `vim` (vi grammar)
· `dev` (editor/IDE ergonomics). or forward ctrl chords to the app instead:
```toml
[input]
passthrough = ["ctrl+w", "ctrl+r"]
```

**is there a scripting language?**

no. use `venti run` (palette commands) + shell for automation.

**network transparency?**

no. local machine only (linux, unix domain sockets).

**plugins / lua / python?**

no. call external scripts via shell.

**copy/paste modes?**

three options:
1. mouse select + middle-click (terminal handles)
2. f9 copy mode (venti selection)
3. osc 52 (app-to-clipboard, if enabled in config)

**how do i detach without killing?**

just close the client. session stays running. reattach with `venti attach -t name`.

**is it fast?**

yes. single-threaded event loop, differential rendering, minimal copying. default frame interval is 8ms (`ui.frame_interval_ms`).

**does it persist state?**

sessions persist. layout persists on clean shutdown. scrollback is 10k lines by
default. `shift+tab` folds the workspace sidebar away in every profile; the
optional `decorations.scroll_indicator` shows how far a pane is scrolled back.

**daemon crashes?**

the server owns the panes, so pane processes end with it. restart with `venti new -s name`; the persisted layout is restored.

**how many panes?**

limited by screen real estate and memory; each pane carries its own vt100 state and scrollback buffer.

**over ssh?**

yes. `ssh user@host venti` works. graphics may work (sixel, kitty) depending on ssh setup.

## troubleshooting

**session stuck:**
```sh
venti kill -t name
venti new -s name
```

**pane lost its images after a full-screen app exits?**
```sh
venti doctor
```

**pane not responding:**
check if app is hung. kill with `f4`.

**graphics not showing:**
```sh
venti doctor
```

graphics is on by default even though it is experimental. if it glitches on
your terminal, turn it off with `graphics = false` in `[general]`.

graphics support is still experimental and is not reliable across every terminal and every layout change. if an image does not show up, or shows up in the wrong place after a resize, a scroll or a float toggle, that is a known rough edge, not a misconfiguration. `docs/graphics-unicode-placeholders.md` describes what venti does with the kitty graphics protocol, sixel and the iterm2 inline image protocol. reproductions and compatibility reports are very welcome, see the graphics protocol support section in the readme.

**config not reloading?**

venti watches the config file; changes apply live. if the file fails to parse,
the old config is kept and a toast shows the error. run `venti config check` to
diagnose. shell, scrollback and the pane and workspace limits apply to new
panes, everything else applies immediately.

**permission denied on socket:**
```sh
venti kill-server
venti
```

## contributing

see [contributing.md](../contributing.md)

## license

mit - see [license](../license)
