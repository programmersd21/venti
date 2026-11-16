# zsh

venti 0.1.0 | 2026-11-16 | linux only

venti runs whatever shell you launch it with; zsh, bash, fish and elvish all
work, and only zsh gets a completion script installed by convention.

## completions

```sh
mkdir -p ~/.zfunc
venti completions zsh > ~/.zfunc/_venti
```

fish users: `venti completions fish > ~/.config/fish/completions/venti.fish`.
the same command works for `bash` and `elvish`; redirect the script into a
directory the shell reads.

in `~/.zshrc`, before `compinit`:

```zsh
fpath=(~/.zfunc $fpath)
autoload -uz compinit && compinit
```

## default shell inside panes

```toml
[general]
shell = "/bin/zsh"
```
