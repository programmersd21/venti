# contributing to venti

venti 0.2.0 | 2026-11-19 | linux only

## workflow

1. open an issue first, discuss before coding
2. fork and branch, `feature/name` or `fix/name`
3. keep commits atomic
4. test before pushing
5. clear pr title, under 70 chars

## development

**setup:**
```sh
git clone https://github.com/programmersd21/venti
cd venti
cargo build
cargo test
```

**commands:**
```sh
cargo build               # debug
cargo build --release     # optimized
cargo test                # all tests
cargo test --lib          # lib tests
cargo fmt --check         # format
cargo clippy --all-targets -- -D warnings
```

**code style:**
- follow existing patterns
- use `cargo fmt`
- pass `cargo clippy` (zero warnings)
- comment *why*, not *what*
- preserve git history

## testing

```sh
cargo test                # all tests
cargo test --lib          # lib tests only
cargo test --no-fail-fast # run everything even if one fails
```

tests should assert observable behavior: what is drawn, what is emitted on
the wire, what a pane receives. add a regression test next to the behavior
it fails in: a `#[test]` in the module, or a `tests/<name>_test.rs`
integration test.

## reporting bugs

**include:**
- venti version: `venti --version`
- os/terminal: `venti doctor`
- reproduction steps
- actual vs expected
- config (from `~/.config/venti/config.toml`)
- log file (if crash): `~/.local/state/venti/venti.log`

## design principles

- **minimal:** fewer features, done well
- **fast:** responsive, efficient
- **modal:** f-keys always do the same thing
- **persistent:** sessions survive detach/restart
- **standard:** follow terminal conventions

## no-go

these won't be accepted:

- gui (terminal-only)
- embedded scripting (use shell/external)
- network multiplexing (local unix domain sockets only)
- async runtime replacement
- per-pane config (session-level only)

## code areas

| area | notes |
|------|-------|
| core loop | event handling, render cycle |
| terminal emu | vt100 parsing, scrollback |
| layout | tiling, floating, workspace |
| client | terminal i/o, rendering |
| config | schema, validation |
| graphics | sixel, kitty |

## documentation

update relevant docs when:
- adding features
- changing keybindings
- modifying config schema
- updating architecture

## questions?

ask in issues. we're friendly.
