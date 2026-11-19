# quickstart

get started with venti in under two minutes.

## installation

### prerequisites
- linux kernel 5.4+
- rust toolchain (edition 2024 / rustc 1.85+)
- terminal emulator; Kitty graphics works in `kitty`, `ghostty`, and `wezterm`; Sixel works in supported terminals such as `foot` and `konsole`

### build from source
```bash
git clone https://github.com/programmersd21/venti
cd venti
cargo build --release
sudo cp target/release/venti /usr/local/bin/
```

## first launch

start an interactive session:
```bash
venti
```

venti automatically starts a background daemon if one is not already running and connects a client to the `default` session.
a default configuration file is automatically generated at `~/.config/venti/config.toml`.

## essential controls

- `f1`: open the help menu and keybinding cheat sheet
- `f2`: open the interactive settings editor (theme, profile, decorations)
- `f11`: open the command palette
- `f12`: open the quick theme switcher

## sessions and management

### named sessions
```bash
# create or attach to a session named "work"
venti new -s work

# list running sessions
venti ls

# attach to a specific session
venti attach -s work

# kill a session
venti kill -t work
```

### system diagnostics
run the system self-check to verify terminal environment, locale, and graphics support:
```bash
venti doctor
```
