//! checked-in completion scripts must match `venti completions <shell>`.
//! regenerate with: `venti completions zsh > completions/_venti` (and bash/fish/elvish).
use std::process::Command;

fn assert_fresh(shell: &str, checked_in: &str) {
    let checked_in = format!("completions/{checked_in}");
    let out = Command::new(env!("CARGO_BIN_EXE_venti"))
        .args(["completions", shell])
        .output()
        .expect("failed to run venti completions");
    assert!(out.status.success(), "venti completions {shell} failed");
    let generated = String::from_utf8(out.stdout).expect("completions are utf-8");
    let checked_in = std::fs::read_to_string(&checked_in)
        .unwrap_or_else(|e| panic!("failed to read {checked_in}: {e}"));
    assert_eq!(
        generated, checked_in,
        "{checked_in} is stale; regenerate with `venti completions {shell} > {checked_in}`"
    );
}

#[test]
fn zsh_completion_is_fresh() {
    assert_fresh("zsh", "_venti");
}

#[test]
fn bash_completion_is_fresh() {
    assert_fresh("bash", "venti.bash");
}

#[test]
fn fish_completion_is_fresh() {
    assert_fresh("fish", "venti.fish");
}

#[test]
fn elvish_completion_is_fresh() {
    assert_fresh("elvish", "venti.elv");
}
