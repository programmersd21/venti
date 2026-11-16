use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Style};
use venti::config::theme::{BUILTIN_THEMES, Theme};
use venti::render::ThemeMenuState;
use venti::render::render_theme_menu;

#[test]
fn theme_menu_is_transparent_and_scrolls_to_selection() {
    let theme = Theme::builtin("mocha").expect("mocha");
    let area = Rect::new(0, 0, 80, 24);
    let mut buf = Buffer::empty(area);
    let bg = Style::default().bg(theme.base.to_ratatui());
    for y in area.y..area.y + area.height {
        for x in area.x..area.x + area.width {
            buf[(x, y)].set_style(bg);
        }
    }

    let state = ThemeMenuState {
        selected_idx: BUILTIN_THEMES.len() - 1,
    };
    render_theme_menu(area, &mut buf, &state, &theme, false);

    // the hero strip previews the *selected* theme's palette
    let sample = Theme::builtin(BUILTIN_THEMES[state.selected_idx]).unwrap();

    let (modal_rect, _) = venti::render::theme_menu::theme_menu_geometry(area);
    let width = modal_rect.width;
    let height = modal_rect.height;
    let start_x = modal_rect.x;
    let start_y = modal_rect.y;

    // the modal's fill is transparent (reset) except the hero palette strip,
    // which intentionally paints the selected theme's colors
    let palette: Vec<_> = [
        sample.base,
        sample.mantle,
        sample.surface,
        sample.overlay,
        sample.muted,
        sample.subtext,
        sample.text,
        sample.accent,
        sample.info,
        sample.success,
        sample.warn,
        sample.alert,
    ]
    .into_iter()
    .map(|c| c.to_ratatui())
    .chain(
        [
            theme.base,
            theme.mantle,
            theme.surface,
            theme.overlay,
            theme.muted,
            theme.subtext,
            theme.text,
            theme.accent,
        ]
        .into_iter()
        .map(|c| c.to_ratatui()),
    ) // grid selection washes use the *current* theme
    .collect();
    for y in start_y..start_y + height {
        for x in start_x..start_x + width {
            let bg = buf[(x, y)].bg;
            assert!(
                bg == Color::Reset || palette.contains(&bg),
                "dialog cell at ({x},{y}) should be transparent or a theme palette color, got {bg:?}"
            );
        }
    }
    // outside cells keep their exact original colors (no repainting with
    // foreign rgb values); dimming overlay is no longer applied for simplicity.
    for (x, y) in [
        (0u16, 0u16),
        (area.width - 1, 0),
        (0, area.height - 1),
        (area.width - 1, area.height - 1),
    ] {
        let cell = &buf[(x, y)];
        assert_eq!(
            cell.bg,
            theme.base.to_ratatui(),
            "outside cell color must be preserved at ({x},{y})"
        );
    }

    let text: String = (start_y..start_y + height)
        .map(|y| {
            (start_x..start_x + width)
                .map(|x| buf[(x, y)].symbol())
                .collect::<String>()
        })
        .collect::<Vec<_>>()
        .join("\n");
    assert!(
        text.contains("vesper"),
        "last theme should always be visible in the grid"
    );
    assert!(
        !BUILTIN_THEMES
            .iter()
            .any(|t| t.contains("light") || *t == "latte"),
        "no light themes should remain"
    );
}
