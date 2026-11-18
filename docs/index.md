# venti documentation

fast, quiet terminal multiplexer for linux.

## contents

- [quickstart](file:///home/programmersd/Projects/github/venti/docs/quickstart.md) - installation and first steps
- [keybindings](file:///home/programmersd/Projects/github/venti/docs/keybindings.md) - complete reference across all 4 profiles
- [configuration](file:///home/programmersd/Projects/github/venti/docs/configuration.md) - configuration reference and live menu settings
- [graphics](file:///home/programmersd/Projects/github/venti/docs/graphics.md) - sixel & kitty graphics protocols, unicode placeholders, setup, and tools
- [architecture](file:///home/programmersd/Projects/github/venti/docs/architecture.md) - daemon, client, and rendering internals
- [protocol](file:///home/programmersd/Projects/github/venti/docs/protocol.md) - ipc wire protocol and serialization
- [zsh integration](file:///home/programmersd/Projects/github/venti/docs/zsh.md) - shell integration and completions

## design principles

- **linux native**: built exclusively for linux using nix syscalls and unix domain sockets.
- **zero bloat**: written in pure rust with minimal dependencies, no c wrappers, and no heavyweight frameworks.
- **dual graphics pipeline**: full support for sixel and kitty graphics protocols (kgp) with unicode placeholders (`U+10EEEE`).
- **rock-solid key handling**: alt-first keybindings avoid stealing critical ctrl-key shortcuts from child apps.
