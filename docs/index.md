# venti documentation

fast, quiet terminal multiplexer for linux.

## contents

- [quickstart](quickstart.md) - installation and first steps
- [keybindings](keybindings.md) - complete reference across all 4 profiles
- [configuration](configuration.md) - configuration reference and live menu settings
- [graphics](graphics.md) - supported graphics protocols, unicode placeholders, setup, and tools
- [architecture](architecture.md) - daemon, client, and rendering internals
- [protocol](protocol.md) - ipc wire protocol and serialization
- [zsh integration](zsh.md) - shell integration and completions

## design principles

- **linux native**: built exclusively for linux using nix syscalls and unix domain sockets.
- **zero bloat**: written in pure rust with minimal dependencies, no c wrappers, and no heavyweight frameworks.
- **graphics pipeline**: sixel and kitty graphics protocol support with unicode placeholders (`U+10EEEE`); iterm2 images are unsupported.
- **rock-solid key handling**: alt-first keybindings avoid stealing critical ctrl-key shortcuts from child apps.
