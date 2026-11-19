use venti::input::{Key, KeyCode, Modifiers};

#[test]
fn encodes_printable_and_control_keys() {
    assert_eq!(Key::plain(KeyCode::Char('a')).to_bytes(), b"a");
    assert_eq!(Key::plain(KeyCode::Char('Z')).to_bytes(), b"Z");
    assert_eq!(Key::ctrl(KeyCode::Char('b')).to_bytes(), vec![0x02]);
    assert_eq!(Key::ctrl(KeyCode::Char('c')).to_bytes(), vec![0x03]);
    assert_eq!(Key::ctrl(KeyCode::Space).to_bytes(), vec![0x00]);
    assert_eq!(Key::alt(KeyCode::Char('x')).to_bytes(), vec![0x1b, b'x']);
    assert_eq!(Key::alt(KeyCode::Enter).to_bytes(), vec![0x1b, b'\r']);
}

#[test]
fn encodes_shifted_printable_keys_with_their_legacy_glyph() {
    let alt_shift = Modifiers {
        ctrl: false,
        alt: true,
        shift: true,
        super_key: false,
    };
    let shift = Modifiers {
        ctrl: false,
        alt: false,
        shift: true,
        super_key: false,
    };

    assert_eq!(Key::new(KeyCode::Char('1'), shift).to_bytes(), b"!");
    assert_eq!(Key::new(KeyCode::Char('1'), alt_shift).to_bytes(), b"\x1b!");
    assert_eq!(Key::new(KeyCode::Char('9'), alt_shift).to_bytes(), b"\x1b(");
    assert_eq!(Key::new(KeyCode::Char(','), alt_shift).to_bytes(), b"\x1b<");
    assert_eq!(Key::new(KeyCode::Char('.'), alt_shift).to_bytes(), b"\x1b>");
    assert_eq!(Key::new(KeyCode::Char('['), alt_shift).to_bytes(), b"\x1b{");
    assert_eq!(Key::new(KeyCode::Char(';'), shift).to_bytes(), b":");
    assert_eq!(Key::new(KeyCode::Char('h'), alt_shift).to_bytes(), b"\x1bH");
}

#[test]
fn encodes_cursor_and_editing_keys() {
    assert_eq!(Key::plain(KeyCode::Up).to_bytes(), b"\x1b[A");
    assert_eq!(Key::plain(KeyCode::Down).to_bytes(), b"\x1b[B");
    assert_eq!(Key::plain(KeyCode::Right).to_bytes(), b"\x1b[C");
    assert_eq!(Key::plain(KeyCode::Left).to_bytes(), b"\x1b[D");
    assert_eq!(Key::plain(KeyCode::Home).to_bytes(), b"\x1b[H");
    assert_eq!(Key::plain(KeyCode::End).to_bytes(), b"\x1b[F");
    assert_eq!(Key::plain(KeyCode::Insert).to_bytes(), b"\x1b[2~");
    assert_eq!(Key::plain(KeyCode::Delete).to_bytes(), b"\x1b[3~");
    assert_eq!(Key::plain(KeyCode::PageUp).to_bytes(), b"\x1b[5~");
    assert_eq!(Key::plain(KeyCode::PageDown).to_bytes(), b"\x1b[6~");
    assert_eq!(Key::plain(KeyCode::Backspace).to_bytes(), vec![0x7f]);
    assert_eq!(Key::plain(KeyCode::Tab).to_bytes(), vec![b'\t']);
}

#[test]
fn encodes_modified_cursor_and_function_keys() {
    assert_eq!(Key::ctrl(KeyCode::Up).to_bytes(), b"\x1b[1;5A");
    assert_eq!(
        Key::new(
            KeyCode::Right,
            Modifiers {
                ctrl: true,
                alt: true,
                shift: true,
                super_key: false,
            },
        )
        .to_bytes(),
        b"\x1b[1;8C"
    );
    assert_eq!(
        Key::new(
            KeyCode::Tab,
            Modifiers {
                ctrl: false,
                alt: false,
                shift: true,
                super_key: false,
            },
        )
        .to_bytes(),
        b"\x1b[1;2Z"
    );
    assert_eq!(Key::plain(KeyCode::F(1)).to_bytes(), b"\x1bOP");
    assert_eq!(Key::plain(KeyCode::F(4)).to_bytes(), b"\x1bOS");
    assert_eq!(Key::plain(KeyCode::F(5)).to_bytes(), b"\x1b[15~");
    assert_eq!(Key::plain(KeyCode::F(12)).to_bytes(), b"\x1b[24~");
}
