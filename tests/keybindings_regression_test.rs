/// comprehensive regression test for all keybindings to ensure no regressions.
use venti::config::schema::ProfileMode;
use venti::input::{Command, InputMode, Key, KeyBindings, KeyCode, Modifiers};

fn alt_shift() -> Modifiers {
    Modifiers {
        ctrl: false,
        alt: true,
        shift: true,
        super_key: false,
    }
}

#[test]
fn test_alt_digits_workspace_goto() {
    let bindings = KeyBindings::new(ProfileMode::Hypr, &[]);

    for i in 1..=9 {
        let ch = char::from_digit(i as u32, 10).unwrap();
        let key = Key::alt(KeyCode::Char(ch));
        assert_eq!(
            bindings.find(InputMode::Normal, &key),
            Some(&Command::WorkspaceGoto(vec![i])),
            "alt+{} should map to WorkspaceGoto(vec![{}])",
            ch,
            i,
        );
    }
}

#[test]
fn test_alt_shift_digits_move_pane() {
    let bindings = KeyBindings::new(ProfileMode::Hypr, &[]);

    for i in 1..=9 {
        let ch = char::from_digit(i as u32, 10).unwrap();
        let key = Key::new(KeyCode::Char(ch), alt_shift());
        assert_eq!(
            bindings.find(InputMode::Normal, &key),
            Some(&Command::MoveToWorkspace(vec![i])),
            "alt+shift+{} should map to MoveToWorkspace(vec![{}])",
            ch,
            i,
        );
    }
}

#[test]
fn test_ctrl_never_bound() {
    let bindings = KeyBindings::new(ProfileMode::Hypr, &[]);

    // alt-first design: plain ctrl chords are never venti's, they reach the app
    for i in 1..=9 {
        let ch = char::from_digit(i as u32, 10).unwrap();
        let key = Key::ctrl(KeyCode::Char(ch));
        assert_eq!(
            bindings.find(InputMode::Normal, &key),
            None,
            "ctrl+{} should not be bound by venti",
            i,
        );
    }
}

#[test]
fn test_alt_hjkl_focus() {
    let bindings = KeyBindings::new(ProfileMode::Hypr, &[]);

    assert_eq!(
        bindings.find(InputMode::Normal, &Key::alt(KeyCode::Char('h'))),
        Some(&Command::FocusLeft),
        "alt+h should be FocusLeft"
    );
    assert_eq!(
        bindings.find(InputMode::Normal, &Key::alt(KeyCode::Char('j'))),
        Some(&Command::FocusDown),
        "alt+j should be FocusDown"
    );
    assert_eq!(
        bindings.find(InputMode::Normal, &Key::alt(KeyCode::Char('k'))),
        Some(&Command::FocusUp),
        "alt+k should be FocusUp"
    );
    assert_eq!(
        bindings.find(InputMode::Normal, &Key::alt(KeyCode::Char('l'))),
        Some(&Command::FocusRight),
        "alt+l should be FocusRight"
    );
}

#[test]
fn test_alt_shift_hjkl_swap() {
    let bindings = KeyBindings::new(ProfileMode::Hypr, &[]);

    assert_eq!(
        bindings.find(
            InputMode::Normal,
            &Key::new(KeyCode::Char('h'), alt_shift())
        ),
        Some(&Command::SwapLeft),
        "alt+shift+h should be SwapLeft"
    );
    assert_eq!(
        bindings.find(
            InputMode::Normal,
            &Key::new(KeyCode::Char('j'), alt_shift())
        ),
        Some(&Command::SwapDown),
        "alt+shift+j should be SwapDown"
    );
    assert_eq!(
        bindings.find(
            InputMode::Normal,
            &Key::new(KeyCode::Char('k'), alt_shift())
        ),
        Some(&Command::SwapUp),
        "alt+shift+k should be SwapUp"
    );
    assert_eq!(
        bindings.find(
            InputMode::Normal,
            &Key::new(KeyCode::Char('l'), alt_shift())
        ),
        Some(&Command::SwapRight),
        "alt+shift+l should be SwapRight"
    );
}

#[test]
fn test_alt_shift_arrows_workspace_move() {
    let bindings = KeyBindings::new(ProfileMode::Hypr, &[]);

    assert_eq!(
        bindings.find(InputMode::Normal, &Key::new(KeyCode::Left, alt_shift())),
        Some(&Command::MoveToWorkspacePrev),
        "alt+shift+left should be MoveToWorkspacePrev"
    );
    assert_eq!(
        bindings.find(InputMode::Normal, &Key::new(KeyCode::Right, alt_shift())),
        Some(&Command::MoveToWorkspaceNext),
        "alt+shift+right should be MoveToWorkspaceNext"
    );
}

#[test]
fn test_alt_basic_commands() {
    let bindings = KeyBindings::new(ProfileMode::Hypr, &[]);

    assert_eq!(
        bindings.find(InputMode::Normal, &Key::alt(KeyCode::Char('n'))),
        Some(&Command::NewWorkspace),
        "alt+n should be NewWorkspace"
    );
    assert_eq!(
        bindings.find(InputMode::Normal, &Key::alt(KeyCode::Char('w'))),
        Some(&Command::CloseWorkspace(vec![])),
        "alt+w should be CloseWorkspace"
    );
    assert_eq!(
        bindings.find(InputMode::Normal, &Key::alt(KeyCode::Char('q'))),
        Some(&Command::ClosePane),
        "alt+q should be ClosePane"
    );
    assert_eq!(
        bindings.find(
            InputMode::Normal,
            &Key::new(KeyCode::Char('q'), alt_shift())
        ),
        Some(&Command::KillSession),
        "alt+shift+q should be KillSession"
    );
}

#[test]
fn test_alt_t_theme_menu() {
    let bindings = KeyBindings::new(ProfileMode::Hypr, &[]);
    assert_eq!(
        bindings.find(InputMode::Normal, &Key::alt(KeyCode::Char('t'))),
        Some(&Command::ThemeMenu),
        "alt+t should be ThemeMenu"
    );
}

#[test]
fn test_alt_s_toggle_sidebar() {
    let bindings = KeyBindings::new(ProfileMode::Hypr, &[]);
    assert_eq!(
        bindings.find(InputMode::Normal, &Key::alt(KeyCode::Char('s'))),
        Some(&Command::ToggleSidebar),
        "alt+s should be ToggleSidebar"
    );
}

#[test]
fn test_alt_space_palette() {
    let bindings = KeyBindings::new(ProfileMode::Hypr, &[]);
    assert_eq!(
        bindings.find(InputMode::Normal, &Key::alt(KeyCode::Space)),
        Some(&Command::CommandPalette),
        "alt+space should be CommandPalette"
    );
}

#[test]
fn test_alt_arrows_workspace_nav() {
    let bindings = KeyBindings::new(ProfileMode::Hypr, &[]);

    assert_eq!(
        bindings.find(InputMode::Normal, &Key::alt(KeyCode::Right)),
        Some(&Command::WorkspaceNext),
        "alt+right should be WorkspaceNext"
    );
    assert_eq!(
        bindings.find(InputMode::Normal, &Key::alt(KeyCode::Left)),
        Some(&Command::WorkspacePrev),
        "alt+left should be WorkspacePrev"
    );
}

#[test]
fn test_alt_comma_and_period_work_as_aliases() {
    let bindings = KeyBindings::new(ProfileMode::Hypr, &[]);

    assert_eq!(
        bindings.find(InputMode::Normal, &Key::alt(KeyCode::Char(','))),
        Some(&Command::WorkspacePrev),
        "alt+, should be WorkspacePrev"
    );
    assert_eq!(
        bindings.find(InputMode::Normal, &Key::alt(KeyCode::Char('.'))),
        Some(&Command::WorkspaceNext),
        "alt+. should be WorkspaceNext"
    );
    assert_eq!(
        bindings.find(
            InputMode::Normal,
            &Key::new(KeyCode::Char(','), alt_shift())
        ),
        Some(&Command::MoveToWorkspacePrev),
        "alt+shift+, should be MoveToWorkspacePrev"
    );
    assert_eq!(
        bindings.find(
            InputMode::Normal,
            &Key::new(KeyCode::Char('.'), alt_shift())
        ),
        Some(&Command::MoveToWorkspaceNext),
        "alt+shift+. should be MoveToWorkspaceNext"
    );
}

#[test]
fn test_alt_tab_not_bound() {
    let bindings = KeyBindings::new(ProfileMode::Hypr, &[]);
    assert_eq!(
        bindings.find(InputMode::Normal, &Key::alt(KeyCode::Tab)),
        None,
        "alt+tab is grabbed by window managers; venti never binds it"
    );
    assert_eq!(
        bindings.find(InputMode::Normal, &Key::new(KeyCode::Tab, alt_shift())),
        None,
        "alt+shift+tab is grabbed by window managers; venti never binds it"
    );
}

#[test]
fn test_alt_shift_n_and_p_not_bound() {
    let bindings = KeyBindings::new(ProfileMode::Hypr, &[]);

    let key_n = Key::new(KeyCode::Char('n'), alt_shift());
    assert_eq!(
        bindings.find(InputMode::Normal, &key_n),
        None,
        "alt+shift+n should not be bound"
    );

    let key_p = Key::new(KeyCode::Char('p'), alt_shift());
    assert_eq!(
        bindings.find(InputMode::Normal, &key_p),
        None,
        "alt+shift+p should not be bound"
    );
}

#[test]
fn test_ctrl_shift_digits_not_bound() {
    let bindings = KeyBindings::new(ProfileMode::Hypr, &[]);

    for i in 1..=9 {
        let ch = char::from_digit(i as u32, 10).unwrap();
        let key = Key::new(
            KeyCode::Char(ch),
            Modifiers {
                ctrl: true,
                alt: false,
                shift: true,
                super_key: false,
            },
        );
        assert_eq!(
            bindings.find(InputMode::Normal, &key),
            None,
            "ctrl+shift+{} should not be bound",
            i
        );
    }
}

#[test]
fn test_fkey_bindings() {
    let bindings = KeyBindings::new(ProfileMode::Hypr, &[]);

    assert_eq!(
        bindings.find(InputMode::Normal, &Key::plain(KeyCode::F(1))),
        Some(&Command::Help),
        "f1 should be Help"
    );
    assert_eq!(
        bindings.find(InputMode::Normal, &Key::plain(KeyCode::F(2))),
        Some(&Command::SettingsMenu),
        "f2 should be SettingsMenu"
    );
    assert_eq!(
        bindings.find(InputMode::Normal, &Key::plain(KeyCode::F(3))),
        Some(&Command::NewPane),
        "f3 should be NewPane"
    );
    assert_eq!(
        bindings.find(InputMode::Normal, &Key::plain(KeyCode::F(4))),
        Some(&Command::ClosePane),
        "f4 should be ClosePane"
    );
    assert_eq!(
        bindings.find(InputMode::Normal, &Key::plain(KeyCode::F(5))),
        Some(&Command::ToggleZoom),
        "f5 should be ToggleZoom"
    );
    assert_eq!(
        bindings.find(InputMode::Normal, &Key::plain(KeyCode::F(6))),
        Some(&Command::ToggleFloat),
        "f6 should be ToggleFloat"
    );
    assert_eq!(
        bindings.find(InputMode::Normal, &Key::plain(KeyCode::F(7))),
        Some(&Command::ToggleScratch),
        "f7 should be ToggleScratch"
    );
    assert_eq!(
        bindings.find(InputMode::Normal, &Key::plain(KeyCode::F(8))),
        Some(&Command::Search),
        "f8 should be Search"
    );
    assert_eq!(
        bindings.find(InputMode::Normal, &Key::plain(KeyCode::F(9))),
        Some(&Command::CopyMode),
        "f9 should be CopyMode"
    );
    assert_eq!(
        bindings.find(InputMode::Normal, &Key::plain(KeyCode::F(10))),
        Some(&Command::ResizeMode),
        "f10 should be ResizeMode"
    );
    assert_eq!(
        bindings.find(InputMode::Normal, &Key::plain(KeyCode::F(11))),
        Some(&Command::CommandPalette),
        "f11 should be CommandPalette"
    );
    assert_eq!(
        bindings.find(InputMode::Normal, &Key::plain(KeyCode::F(12))),
        Some(&Command::ThemeMenu),
        "f12 should be ThemeMenu"
    );
}

#[test]
fn test_alt_p_rotate_split_in_all_profiles() {
    // rotate split works out of the box in every profile
    for profile in [ProfileMode::Hypr, ProfileMode::Hypr] {
        let bindings = KeyBindings::new(profile, &[]);
        assert_eq!(
            bindings.find(InputMode::Normal, &Key::alt(KeyCode::Char('p'))),
            Some(&Command::RotateSplit),
            "alt+p should be RotateSplit in {:?}",
            profile
        );
    }
}

#[test]
fn test_passthrough_overrides_binding() {
    let passthrough = vec!["alt+h".to_string()];
    let bindings = KeyBindings::new(ProfileMode::Hypr, &passthrough);

    // alt+h should not be bound when it's in passthrough list
    assert_eq!(
        bindings.find(InputMode::Normal, &Key::alt(KeyCode::Char('h'))),
        None,
        "alt+h should not bind when in passthrough list"
    );

    // but alt+j should still work
    assert_eq!(
        bindings.find(InputMode::Normal, &Key::alt(KeyCode::Char('j'))),
        Some(&Command::FocusDown),
        "alt+j should still bind"
    );
}

#[test]
fn test_disabled_keys_override_bindings() {
    let mut bindings = KeyBindings::new(ProfileMode::Hypr, &[]);
    bindings.set_disabled(&["alt+q".to_string()]);

    // alt+q should not be found when it's disabled
    assert_eq!(
        bindings.find(InputMode::Normal, &Key::alt(KeyCode::Char('q'))),
        None,
        "alt+q should not bind when disabled"
    );
}

#[test]
fn test_beginner_binds_arrows_focus_and_not_hjkl() {
    let bindings = KeyBindings::new(ProfileMode::Beginner, &[]);
    assert_eq!(
        bindings.find(InputMode::Normal, &Key::alt(KeyCode::Left)),
        Some(&Command::FocusLeft)
    );
    assert_eq!(
        bindings.find(InputMode::Normal, &Key::alt(KeyCode::Char('h'))),
        None,
        "beginner must not bind hjkl; arrows are the focus path"
    );
    // f-keys always available
    assert_eq!(
        bindings.find(InputMode::Normal, &Key::plain(KeyCode::F(3))),
        Some(&Command::NewPane)
    );
}

#[test]
fn test_vim_binds_vi_grammar() {
    let bindings = KeyBindings::new(ProfileMode::Vim, &[]);
    assert_eq!(
        bindings.find(InputMode::Normal, &Key::alt(KeyCode::Char('['))),
        Some(&Command::WorkspacePrev)
    );
    assert_eq!(
        bindings.find(InputMode::Normal, &Key::alt(KeyCode::Char(']'))),
        Some(&Command::WorkspaceNext)
    );
    assert_eq!(
        bindings.find(InputMode::Normal, &Key::alt(KeyCode::Char('x'))),
        Some(&Command::ClosePane)
    );
    assert_eq!(
        bindings.find(InputMode::Normal, &Key::alt(KeyCode::Char('-'))),
        Some(&Command::ResizeLeft)
    );
    assert_eq!(
        bindings.find(InputMode::Normal, &Key::alt(KeyCode::Char('='))),
        Some(&Command::ResizeRight)
    );
    assert_eq!(
        bindings.find(InputMode::Normal, &Key::alt(KeyCode::Char('p'))),
        Some(&Command::CommandPalette)
    );
    assert_eq!(
        bindings.find(InputMode::Normal, &Key::alt(KeyCode::Char('e'))),
        Some(&Command::ThemeMenu)
    );
    // hjkl focus retained
    assert_eq!(
        bindings.find(InputMode::Normal, &Key::alt(KeyCode::Char('h'))),
        Some(&Command::FocusLeft)
    );
}

#[test]
fn test_dev_binds_editor_ergonomics() {
    let bindings = KeyBindings::new(ProfileMode::Dev, &[]);
    assert_eq!(
        bindings.find(InputMode::Normal, &Key::alt(KeyCode::Enter)),
        Some(&Command::NewPane)
    );
    assert_eq!(
        bindings.find(InputMode::Normal, &Key::alt(KeyCode::Char('k'))),
        Some(&Command::CommandPalette)
    );
    assert_eq!(
        bindings.find(InputMode::Normal, &Key::alt(KeyCode::Left)),
        Some(&Command::FocusLeft)
    );
    // hjkl unbound: raw bytes reach the app
    assert_eq!(
        bindings.find(InputMode::Normal, &Key::alt(KeyCode::Char('h'))),
        None
    );
    assert_eq!(
        bindings.find(InputMode::Normal, &Key::alt(KeyCode::Char('t'))),
        Some(&Command::ThemeMenu)
    );
}

#[test]
fn test_profiles_are_genuinely_distinct() {
    let beginner = KeyBindings::new(ProfileMode::Beginner, &[]);
    let hypr = KeyBindings::new(ProfileMode::Hypr, &[]);
    let vim = KeyBindings::new(ProfileMode::Vim, &[]);
    let dev = KeyBindings::new(ProfileMode::Dev, &[]);

    // arrows mean workspace cycling in hypr, focus in beginner/dev, unbound in vim
    assert_eq!(
        hypr.find(InputMode::Normal, &Key::alt(KeyCode::Right)),
        Some(&Command::WorkspaceNext)
    );
    assert_eq!(
        beginner.find(InputMode::Normal, &Key::alt(KeyCode::Right)),
        Some(&Command::FocusRight)
    );
    assert_eq!(vim.find(InputMode::Normal, &Key::alt(KeyCode::Right)), None);
    assert_eq!(
        dev.find(InputMode::Normal, &Key::alt(KeyCode::Right)),
        Some(&Command::FocusRight)
    );

    // alt+space only in hypr; vim/dev use their own chords; beginner has none
    assert_eq!(
        hypr.find(InputMode::Normal, &Key::alt(KeyCode::Space)),
        Some(&Command::CommandPalette)
    );
    assert_eq!(
        beginner.find(InputMode::Normal, &Key::alt(KeyCode::Space)),
        None
    );
    assert_eq!(vim.find(InputMode::Normal, &Key::alt(KeyCode::Space)), None);
    assert_eq!(dev.find(InputMode::Normal, &Key::alt(KeyCode::Space)), None);

    // every profile still binds f1 and digits
    for profile in [
        ProfileMode::Beginner,
        ProfileMode::Hypr,
        ProfileMode::Vim,
        ProfileMode::Dev,
    ] {
        let bindings = KeyBindings::new(profile, &[]);
        assert_eq!(
            bindings.find(InputMode::Normal, &Key::plain(KeyCode::F(1))),
            Some(&Command::Help)
        );
        assert_eq!(
            bindings.find(InputMode::Normal, &Key::alt(KeyCode::Char('3'))),
            Some(&Command::WorkspaceGoto(vec![3]))
        );
    }
}
