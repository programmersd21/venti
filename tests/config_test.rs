#[cfg(test)]
mod tests {
    use venti::config::theme::BUILTIN_THEMES;
    use venti::config::{RgbColor, Theme, default_config_commented, load_from_str};

    #[test]
    fn empty_config_is_rejected_not_silently_defaulted() {
        for empty in ["", "   \n\t\n"] {
            let errors = load_from_str(empty).expect_err("empty config must be an error");
            assert!(errors.iter().any(|e| e.message.contains("empty")));
        }
    }

    #[test]
    fn legacy_animation_keys_are_migrated_on_load() {
        let legacy = r#"
[animation]
enabled = true
duration_ms = 140
easing = "ease_out_cubic"
workspace_duration_ms = 250
workspace_easing = "ease_out_cubic"

[general]
graphics_unicode_placeholders = true

[ui]
workspace_bar = "left"
"#;
        let config = load_from_str(legacy).expect("legacy config must load");
        assert_eq!(config.animation.layout_duration_ms, 140);
        assert_eq!(
            config.animation.layout_easing,
            venti::config::schema::EasingType::EaseOutCubic
        );
        assert_eq!(
            config.ui.workspace_bar,
            venti::config::schema::WorkspaceBarPosition::Left
        );
    }

    #[test]
    fn new_animation_keys_win_when_both_are_present() {
        let mixed = r#"
[animation]
duration_ms = 140
layout_duration_ms = 75
layout_easing = "linear"
"#;
        let config = load_from_str(mixed).expect("mixed config must load");
        assert_eq!(config.animation.layout_duration_ms, 75);
    }

    #[test]
    fn test_builtin_themes() {
        let mocha = Theme::builtin("mocha").expect("mocha exists");
        assert_eq!(mocha.name, "mocha");
        assert_eq!(mocha.base, RgbColor::new(0x1e, 0x1e, 0x2e));
        assert_eq!(mocha.accent, RgbColor::new(0xcb, 0xa6, 0xf7));

        let tokyo = Theme::builtin("tokyo-night").expect("tokyo-night exists");
        assert_eq!(tokyo.name, "tokyo-night");

        let gruvbox = Theme::builtin("gruvbox-dark").expect("gruvbox-dark exists");
        assert_eq!(gruvbox.name, "gruvbox-dark");
    }

    #[test]
    fn test_every_builtin_theme_resolves() {
        assert_eq!(BUILTIN_THEMES.len(), 15);
        let mut seen = std::collections::HashSet::new();
        for name in BUILTIN_THEMES {
            assert!(seen.insert(*name), "duplicate theme name: {name}");
            let theme = Theme::builtin(name).unwrap_or_else(|| panic!("missing theme: {name}"));
            assert_eq!(theme.name, *name);
            assert!(theme.is_dark(), "{name} must be a dark theme");
        }
    }

    #[test]
    fn test_color_blend_and_quantize() {
        let black = RgbColor::new(0, 0, 0);
        let white = RgbColor::new(255, 255, 255);
        let mid = black.blend(white, 0.5);
        assert_eq!(mid, RgbColor::new(128, 128, 128));

        // Quantization tests
        let q_black = black.quantize_256();
        assert_eq!(q_black, 16);
        let q_white = white.quantize_256();
        assert_eq!(q_white, 231);
    }

    #[test]
    fn test_load_default_commented_config() {
        let def = default_config_commented();
        let config = load_from_str(def).expect("default commented config must parse and validate");
        assert_eq!(config.general.scrollback, 10000);
        assert_eq!(config.ui.frame_interval_ms, 8);
    }

    #[test]
    fn test_animation_and_input_keys_are_recognized() {
        // the schema added these keys after the validator did; they must not
        // be reported as unknown (regression: daemon refused to start with a
        // valid config containing them)
        let toml = r#"
[input]
passthrough = []
scroll_lines = 3
disabled_keys = ["ctrl+shift+q"]

[animation]
enabled = true
layout_duration_ms = 400
layout_easing = "ease_out_quint"
"#;
        let config = load_from_str(toml).expect("config with animation/input keys must validate");
        assert_eq!(config.input.disabled_keys, vec!["ctrl+shift+q".to_string()]);
        assert_eq!(config.animation.layout_duration_ms, 400);
    }

    #[test]
    fn test_validation_errors() {
        let invalid_toml = r#"
[decorations]
dim_strength = 2.5

[ui]
frame_interval_ms = 2

[unknown_section]
foo = "bar"
"#;
        let errs = load_from_str(invalid_toml).unwrap_err();
        assert!(errs.iter().any(|e| e.message.contains("dim_strength")));
        assert!(errs.iter().any(|e| e.message.contains("frame_interval_ms")));
        assert!(errs.iter().any(|e| e.message.contains("unknown section")));
    }
}
