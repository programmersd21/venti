use proptest::prelude::*;
use venti::config::schema::ProfileMode;
use venti::input::{
    Command, InputMode, Key, KeyBindings, KeyCode, KeyParser, Modifiers, ParsedInput, parse_key,
};

#[test]
fn test_parse_key_strings() {
    assert_eq!(parse_key("alt+enter").unwrap(), Key::alt(KeyCode::Enter));
    assert_eq!(parse_key("alt+b").unwrap(), Key::alt(KeyCode::Char('b')));
    assert_eq!(
        parse_key("ctrl+shift+h").unwrap(),
        Key::new(
            KeyCode::Char('h'),
            Modifiers {
                ctrl: true,
                alt: false,
                shift: true,
                super_key: false
            }
        )
    );
    assert_eq!(parse_key("f1").unwrap(), Key::plain(KeyCode::F(1)));
}

#[test]
fn test_hypr_bindings() {
    let bindings = KeyBindings::new(ProfileMode::Hypr, &[]);
    assert_eq!(
        bindings.find(InputMode::Normal, &Key::plain(KeyCode::F(3))),
        Some(&Command::NewPane)
    );
    assert_eq!(
        bindings.find(InputMode::Normal, &Key::plain(KeyCode::F(4))),
        Some(&Command::ClosePane)
    );
    assert_eq!(
        bindings.find(InputMode::Normal, &Key::alt(KeyCode::Space)),
        Some(&Command::CommandPalette)
    );
    assert_eq!(
        bindings.find(
            InputMode::Normal,
            &Key::new(
                KeyCode::Char('q'),
                Modifiers {
                    ctrl: false,
                    alt: true,
                    shift: true,
                    super_key: false,
                },
            ),
        ),
        Some(&Command::KillSession)
    );
    assert_eq!(
        bindings.find(InputMode::Normal, &Key::alt(KeyCode::Char('1'))),
        Some(&Command::WorkspaceGoto(vec![1]))
    );
    assert_eq!(
        bindings.find(InputMode::Normal, &Key::alt(KeyCode::Char('9'))),
        Some(&Command::WorkspaceGoto(vec![9]))
    );
    assert_eq!(
        bindings.find(
            InputMode::Normal,
            &Key::new(
                KeyCode::Char('1'),
                Modifiers {
                    ctrl: false,
                    alt: true,
                    shift: true,
                    super_key: false,
                }
            )
        ),
        Some(&Command::MoveToWorkspace(vec![1]))
    );
    assert_eq!(
        bindings.find(
            InputMode::Normal,
            &Key::new(
                KeyCode::Char('2'),
                Modifiers {
                    ctrl: false,
                    alt: true,
                    shift: true,
                    super_key: false,
                }
            )
        ),
        Some(&Command::MoveToWorkspace(vec![2]))
    );
    assert_eq!(
        bindings.find(InputMode::Normal, &Key::alt(KeyCode::Char('n'))),
        Some(&Command::NewWorkspace)
    );
    assert_eq!(
        bindings.find(InputMode::Normal, &Key::alt(KeyCode::Char('w'))),
        Some(&Command::CloseWorkspace(vec![]))
    );
}

#[test]
fn test_quit_command_alias() {
    assert_eq!("quit".parse::<Command>().unwrap(), Command::KillSession);
    assert_eq!(
        "kill_session".parse::<Command>().unwrap(),
        Command::KillSession
    );
}

#[test]
fn test_key_parser_sequences() {
    let mut parser = KeyParser::new();

    // Plain ascii
    let ev1 = parser.parse_bytes(b"a");
    assert_eq!(ev1, vec![ParsedInput::Key(Key::plain(KeyCode::Char('a')))]);

    // Alt+Enter (\x1b\r)
    let ev2 = parser.parse_bytes(b"\x1b\r");
    assert_eq!(ev2, vec![ParsedInput::Key(Key::alt(KeyCode::Enter))]);

    // Arrow keys (\x1b[A)
    let ev3 = parser.parse_bytes(b"\x1b[A");
    assert_eq!(ev3, vec![ParsedInput::Key(Key::plain(KeyCode::Up))]);

    // Bracketed paste
    let ev4 = parser.parse_bytes(b"\x1b[200~hello world\x1b[201~");
    assert_eq!(ev4, vec![ParsedInput::Paste(b"hello world".to_vec())]);

    // Alt+shift+letter and alt+ctrl+letter arrive as ESC + upper/control byte.
    let ev5 = parser.parse_bytes(b"\x1bH");
    assert_eq!(
        ev5,
        vec![ParsedInput::Key(Key::new(
            KeyCode::Char('h'),
            Modifiers {
                ctrl: false,
                alt: true,
                shift: true,
                super_key: false,
            },
        ))]
    );
    let ev6 = parser.parse_bytes(b"\x1b\x11");
    assert_eq!(
        ev6,
        vec![ParsedInput::Key(Key::new(
            KeyCode::Char('q'),
            Modifiers {
                ctrl: true,
                alt: true,
                shift: false,
                super_key: false,
            },
        ))]
    );
    let ev7 = parser.parse_bytes(b"\x1b!");
    assert_eq!(
        ev7,
        vec![ParsedInput::Key(Key::new(
            KeyCode::Char('1'),
            Modifiers {
                ctrl: false,
                alt: true,
                shift: true,
                super_key: false,
            },
        ))]
    );
    // Test alt+shift+2 (@)
    let ev8 = parser.parse_bytes(b"\x1b@");
    assert_eq!(
        ev8,
        vec![ParsedInput::Key(Key::new(
            KeyCode::Char('2'),
            Modifiers {
                ctrl: false,
                alt: true,
                shift: true,
                super_key: false,
            },
        ))]
    );
    // Test alt+shift+3 (#)
    let ev9 = parser.parse_bytes(b"\x1b#");
    assert_eq!(
        ev9,
        vec![ParsedInput::Key(Key::new(
            KeyCode::Char('3'),
            Modifiers {
                ctrl: false,
                alt: true,
                shift: true,
                super_key: false,
            },
        ))]
    );
}

#[test]
fn oversized_paste_is_discarded_through_terminator() {
    let mut parser = KeyParser::new();
    let mut paste = b"\x1b[200~".to_vec();
    paste.extend(std::iter::repeat_n(b'x', 256 * 1024 + 1));
    paste.extend_from_slice(b"q\x1b[201~z");

    let events = parser.parse_bytes(&paste);

    assert_eq!(
        events,
        vec![ParsedInput::Key(Key::plain(KeyCode::Char('z')))]
    );
}

#[test]
fn oversized_paste_detects_terminator_started_at_limit() {
    let mut parser = KeyParser::new();
    let mut paste = b"\x1b[200~".to_vec();
    paste.extend(std::iter::repeat_n(b'x', 256 * 1024));
    paste.extend_from_slice(b"\x1b[201~z");

    assert_eq!(
        parser.parse_bytes(&paste),
        vec![ParsedInput::Key(Key::plain(KeyCode::Char('z')))]
    );
}

#[test]
fn escape_prefix_waits_for_split_sequence_or_timeout() {
    let mut parser = KeyParser::new();
    assert!(parser.parse_bytes(b"\x1b").is_empty());
    assert!(parser.has_pending_escape());
    assert_eq!(
        parser.parse_bytes(b"[A"),
        vec![ParsedInput::Key(Key::plain(KeyCode::Up))]
    );
    assert!(!parser.has_pending_escape());

    assert!(parser.parse_bytes(b"\x1b").is_empty());
    assert_eq!(
        parser.flush_pending_escape(),
        Some(ParsedInput::Key(Key::plain(KeyCode::Esc)))
    );
}

proptest! {
    #[test]
    fn test_parser_never_panics_on_arbitrary_bytes(bytes in proptest::collection::vec(any::<u8>(), 0..1024)) {
        let mut parser = KeyParser::new();
        let _ = parser.parse_bytes(&bytes);
    }
}
