use venti::input::{Key, KeyCode, KeyParser, Modifiers, ParsedInput};

fn parse(bytes: &[u8]) -> Vec<ParsedInput> {
    KeyParser::new().parse_bytes(bytes)
}

#[test]
fn plain_f1_f4_parse_as_keys() {
    assert_eq!(
        parse(b"\x1b[P"),
        vec![ParsedInput::Key(Key::plain(KeyCode::F(1)))]
    );
    assert_eq!(
        parse(b"\x1b[Q"),
        vec![ParsedInput::Key(Key::plain(KeyCode::F(2)))]
    );
    assert_eq!(
        parse(b"\x1b[R"),
        vec![ParsedInput::Key(Key::plain(KeyCode::F(3)))]
    );
    assert_eq!(
        parse(b"\x1b[S"),
        vec![ParsedInput::Key(Key::plain(KeyCode::F(4)))]
    );
    // application-mode F1-F4 (SS3)
    assert_eq!(
        parse(b"\x1bOP"),
        vec![ParsedInput::Key(Key::plain(KeyCode::F(1)))]
    );
    assert_eq!(
        parse(b"\x1bOQ"),
        vec![ParsedInput::Key(Key::plain(KeyCode::F(2)))]
    );
}

#[test]
fn modified_f1_f4_parse_with_modifiers() {
    assert_eq!(
        parse(b"\x1b[1;5P"),
        vec![ParsedInput::Key(Key::new(
            KeyCode::F(1),
            Modifiers {
                ctrl: true,
                ..Modifiers::NONE
            }
        ))]
    );
    assert_eq!(
        parse(b"\x1b[1;2S"),
        vec![ParsedInput::Key(Key::new(
            KeyCode::F(4),
            Modifiers {
                shift: true,
                ..Modifiers::NONE
            }
        ))]
    );
}

#[test]
fn modify_other_keys_background() {
    // xterm modifyOtherKeys: CSI 27 ; <mods> ; <code> ~
    assert_eq!(
        parse(b"\x1b[27;5;113~"),
        vec![ParsedInput::Key(Key::new(
            KeyCode::Char('q'),
            Modifiers {
                ctrl: true,
                ..Modifiers::NONE
            }
        ))]
    );
    // ctrl+shift+4 (codepoint 52 = '4')
    assert_eq!(
        parse(b"\x1b[27;6;52~"),
        vec![ParsedInput::Key(Key::new(
            KeyCode::Char('4'),
            Modifiers {
                ctrl: true,
                shift: true,
                ..Modifiers::NONE
            }
        ))]
    );
    // f2 with ctrl
    assert_eq!(
        parse(b"\x1b[27;5;12~"),
        vec![ParsedInput::Key(Key::new(
            KeyCode::F(2),
            Modifiers {
                ctrl: true,
                ..Modifiers::NONE
            }
        ))]
    );
}

#[test]
fn kitty_functional_keys() {
    // kitty: F1..F12 are legacy-encoded even under flag 1, F13+ are PUA codepoints
    assert_eq!(
        parse(b"\x1b[57376u"),
        vec![ParsedInput::Key(Key::plain(KeyCode::F(13)))]
    );
    assert_eq!(
        parse(b"\x1b[57380;2u"),
        vec![ParsedInput::Key(Key::new(
            KeyCode::F(17),
            Modifiers {
                shift: true,
                ..Modifiers::NONE
            }
        ))]
    );
}

#[test]
fn kitty_ctrl_letters_and_shift_digits() {
    // ctrl+q as kitty sends it under flag 1
    assert_eq!(
        parse(b"\x1b[113;5u"),
        vec![ParsedInput::Key(Key::new(
            KeyCode::Char('q'),
            Modifiers {
                ctrl: true,
                ..Modifiers::NONE
            }
        ))]
    );
    // ctrl+shift+4 -> '4' with ctrl+shift (from codepoint 52)
    assert_eq!(
        parse(b"\x1b[52;6u"),
        vec![ParsedInput::Key(Key::new(
            KeyCode::Char('4'),
            Modifiers {
                ctrl: true,
                shift: true,
                ..Modifiers::NONE
            }
        ))]
    );
    // shift+$ -> normalized to '4' with shift
    assert_eq!(
        parse(b"\x1b[36;2u"),
        vec![ParsedInput::Key(Key::new(
            KeyCode::Char('4'),
            Modifiers {
                shift: true,
                ..Modifiers::NONE
            }
        ))]
    );
}

#[test]
fn kitty_plain_text_unaffected() {
    // plain letters still arrive as raw utf8, no modifiers
    assert_eq!(
        parse(b"a"),
        vec![ParsedInput::Key(Key::plain(KeyCode::Char('a')))]
    );
}

#[test]
fn outer_terminal_apc_responses_swallowed() {
    // kitty's reply to an a=q probe lands on stdin as an APC sequence; it
    // must not leak to the pane as stray keystrokes.
    assert_eq!(parse(b"\x1b_Gi=3;OK\x1b\\"), vec![]);
    // same with payload splitting across the parser's push order
    assert_eq!(parse(b"\x1b_Ga=T;AAAA\x1b\\\x1b_Gi=4;OK\x1b\\"), vec![]);
    // text before and after is unaffected
    assert_eq!(
        parse(b"a\x1b_Gi=3;OK\x1b\\b"),
        vec![
            ParsedInput::Key(Key::plain(KeyCode::Char('a'))),
            ParsedInput::Key(Key::plain(KeyCode::Char('b'))),
        ]
    );
}

#[test]
fn outer_terminal_dcs_responses_swallowed() {
    // DCS XTGETTCAP / XTVERSION replies
    assert_eq!(parse(b"\x1bP1+r4669;i\x1b\\"), vec![]);
    assert_eq!(parse(b"\x1bP>|kitty(0.40.1)\x1b\\"), vec![]);
    assert_eq!(
        parse(b"x\x1bP1+r4669;i\x1b\\y"),
        vec![
            ParsedInput::Key(Key::plain(KeyCode::Char('x'))),
            ParsedInput::Key(Key::plain(KeyCode::Char('y'))),
        ]
    );
}

#[test]
fn outer_terminal_osc_responses_swallowed() {
    assert_eq!(parse(b"\x1b]4;1;rgb:ff/ff/ff\x07"), vec![]);
    assert_eq!(parse(b"\x1b]4;1;rgb:ff/ff/ff\x1b\\"), vec![]);
    assert_eq!(
        parse(b"\x1b]4;1;rgb:ff/ff/ff\x07q"),
        vec![ParsedInput::Key(Key::plain(KeyCode::Char('q')))]
    );
}

#[test]
fn kitty_ctrl_shift_arrows() {
    // ctrl+shift+up/down/left/right via xterm-style CSI
    assert_eq!(
        parse(b"\x1b[1;6A"),
        vec![ParsedInput::Key(Key::new(
            KeyCode::Up,
            Modifiers {
                ctrl: true,
                shift: true,
                ..Modifiers::NONE
            }
        ))]
    );
    assert_eq!(
        parse(b"\x1b[1;6B"),
        vec![ParsedInput::Key(Key::new(
            KeyCode::Down,
            Modifiers {
                ctrl: true,
                shift: true,
                ..Modifiers::NONE
            }
        ))]
    );
    assert_eq!(
        parse(b"\x1b[1;6D"),
        vec![ParsedInput::Key(Key::new(
            KeyCode::Left,
            Modifiers {
                ctrl: true,
                shift: true,
                ..Modifiers::NONE
            }
        ))]
    );
    assert_eq!(
        parse(b"\x1b[1;6C"),
        vec![ParsedInput::Key(Key::new(
            KeyCode::Right,
            Modifiers {
                ctrl: true,
                shift: true,
                ..Modifiers::NONE
            }
        ))]
    );
}
