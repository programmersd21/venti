# zsh integration

instructions and tips for running venti with zsh.

## shell setup

add the following to `~/.zshrc` to ensure correct terminal environment inside venti:

```zsh
# disable prompt redrawing on terminal resize if your theme flickers
if [[ -n "$VENTI" ]]; then
  # running inside a venti pane
  export TERM="${TERM:-xterm-256color}"
fi
```

## shell completions

venti can generate native shell completion scripts via `clap_complete`.

### generating zsh completions
```zsh
mkdir -p ~/.zsh/completions
venti completion zsh > ~/.zsh/completions/_venti
```

add to `~/.zshrc` (before `compinit`):
```zsh
fpath=(~/.zsh/completions $fpath)
autoload -Uz compinit
compinit
```

## quick aliases

recommended shortcuts in `~/.zshrc`:
```zsh
alias v="venti"
alias vn="venti new -s"
alias vls="venti ls"
alias va="venti attach -s"
alias vk="venti kill -t"
```
