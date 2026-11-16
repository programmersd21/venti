use venti::config::schema::ProfileMode;
use venti::input::{InputMode, Key, KeyBindings, KeyCode, KeyParser, ParsedInput};

#[test]
fn kitty_ctrl_tab_parses_as_ctrl_tab() {
    let mut p = KeyParser::new();
    let out = p.parse_bytes(b"\x1b[9;5u");
    assert_eq!(out.len(), 1);
    let ParsedInput::Key(k) = &out[0] else {
        panic!(
            "expected kitty ctrl+tab to parse as a key, got {:?}",
            out[0]
        );
    };
    assert_eq!(k.code, KeyCode::Tab);
    assert!(k.modifiers.ctrl);
    assert!(!k.modifiers.shift);
}

#[test]
fn alt_tab_is_unbound_by_design() {
    let bindings = KeyBindings::new(ProfileMode::Beginner, &[]);
    // alt+tab is the window-manager switcher on most desktops; venti never binds it
    assert_eq!(
        bindings.find(InputMode::Normal, &Key::alt(KeyCode::Tab)),
        None
    );
}
