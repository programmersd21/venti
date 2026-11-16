use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use venti::config::schema::ProfileMode;
use venti::config::theme::Theme;
use venti::render::render_help;

fn rendered_lines(buffer: &Buffer) -> Vec<String> {
    (0..buffer.area.height)
        .map(|y| {
            (0..buffer.area.width)
                .map(|x| buffer[(x, y)].symbol())
                .collect::<String>()
        })
        .collect()
}

#[test]
fn help_overlay_lists_current_workspace_keybindings() {
    let theme = Theme::builtin("mocha").expect("mocha");
    let mut buffer = Buffer::empty(Rect::new(0, 0, 100, 45));

    render_help(
        buffer.area,
        &mut buffer,
        &theme,
        false,
        0,
        ProfileMode::Hypr,
    );
    let screen = rendered_lines(&buffer).join("\n");

    for text in [
        "alt+1..9",
        "alt+shift+h",
        "alt+right",
        "f12",
        "command palette",
    ] {
        assert!(screen.contains(text), "missing {text:?} from:\n{screen}");
    }
}

#[test]
fn help_overlay_scrolls_without_panicking() {
    let theme = Theme::builtin("mocha").expect("mocha");
    let mut buffer = Buffer::empty(Rect::new(0, 0, 100, 45));

    // scroll past the end; must clamp instead of panicking
    render_help(
        buffer.area,
        &mut buffer,
        &theme,
        false,
        10_000,
        ProfileMode::Hypr,
    );
    let screen = rendered_lines(&buffer).join("\n");
    assert!(
        screen.contains("command palette"),
        "scrolled help still shows the end:\n{screen}"
    );
}

#[test]
fn help_overlay_ascii_mode_renders() {
    let theme = Theme::builtin("mocha").expect("mocha");
    let mut buffer = Buffer::empty(Rect::new(0, 0, 100, 45));

    render_help(buffer.area, &mut buffer, &theme, true, 0, ProfileMode::Hypr);
    let screen = rendered_lines(&buffer).join("\n");
    assert!(
        screen.contains("alt+1"),
        "ascii help still readable:\n{screen}"
    );
}

fn screen_contains(theme: &Theme, profile: ProfileMode, texts: &[&str]) -> bool {
    let mut buffer = Buffer::empty(Rect::new(0, 0, 100, 45));
    render_help(buffer.area, &mut buffer, theme, false, 0, profile);
    let screen = rendered_lines(&buffer).join("\n");
    texts.iter().all(|t| screen.contains(t))
}

#[test]
fn help_shows_profile_specific_focus_rows() {
    let theme = Theme::builtin("mocha").expect("mocha");
    assert!(
        screen_contains(
            &theme,
            ProfileMode::Beginner,
            &["alt+left", "alt+right", "focus left", "focus right"],
        ),
        "beginner help must list alt+arrows focus"
    );
    assert!(
        screen_contains(
            &theme,
            ProfileMode::Hypr,
            &["alt+h", "alt+shift+h", "swap left"],
        ),
        "hypr help must list hjkl focus + swap"
    );
    assert!(
        screen_contains(&theme, ProfileMode::Vim, &["alt+-", "alt+=", "resize pane"],),
        "vim help must list -/= resize"
    );
    assert!(
        screen_contains(&theme, ProfileMode::Dev, &["alt+enter", "new pane",]),
        "dev help must list alt+enter new pane"
    );
}

#[test]
fn help_title_names_the_profile() {
    let theme = Theme::builtin("mocha").expect("mocha");
    for (profile, expected) in [
        (ProfileMode::Beginner, "beginner"),
        (ProfileMode::Hypr, "hypr"),
        (ProfileMode::Vim, "vim"),
        (ProfileMode::Dev, "dev"),
    ] {
        let mut buffer = Buffer::empty(Rect::new(0, 0, 100, 45));
        render_help(buffer.area, &mut buffer, &theme, false, 0, profile);
        let screen = rendered_lines(&buffer).join("\n");
        assert!(
            screen.contains(expected),
            "help title must name profile {expected}:\n{screen}"
        );
    }
}

#[test]
fn help_lists_profile_palette_chord() {
    let theme = Theme::builtin("mocha").expect("mocha");
    let mut buffer = Buffer::empty(Rect::new(0, 0, 100, 45));
    render_help(
        buffer.area,
        &mut buffer,
        &theme,
        false,
        usize::MAX / 2,
        ProfileMode::Hypr,
    );
    let screen = rendered_lines(&buffer).join("\n");
    assert!(
        screen.contains("alt+space") && screen.contains("command palette"),
        "hypr help must list alt+space command palette:\n{screen}"
    );
    // hypr-only: vim's alt+p is palette, not rotate; section should not
    // claim alt+p rotated anything for vim
    let mut buffer = Buffer::empty(Rect::new(0, 0, 100, 45));
    render_help(
        buffer.area,
        &mut buffer,
        &theme,
        false,
        usize::MAX / 2,
        ProfileMode::Vim,
    );
    let screen = rendered_lines(&buffer).join("\n");
    assert!(
        screen.contains("alt+p") && screen.contains("palette"),
        "vim help must list alt+p palette:\n{screen}"
    );
}
