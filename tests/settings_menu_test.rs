use unicode_width::UnicodeWidthStr;
use venti::config::schema::Config;
use venti::render::settings_menu::{Setting, SettingTab, SettingsMenuState};

/// every setting's name must fit the 30-cell name column.
#[test]
fn setting_names_fit_the_name_column() {
    for tab in [
        SettingTab::Quick,
        SettingTab::Visuals,
        SettingTab::Layout,
        SettingTab::Advanced,
    ] {
        for setting in tab.filtered_settings() {
            let w = UnicodeWidthStr::width(setting.name());
            assert!(w <= 30, "{} name is {} cells wide", setting.name(), w);
        }
    }
}

/// every category must fit the 11-cell category column.
#[test]
fn setting_categories_fit_the_category_column() {
    for setting in Setting::all() {
        let w = UnicodeWidthStr::width(setting.category());
        assert!(
            w <= 11,
            "{} category {:?} is {} cells wide",
            setting.name(),
            setting.category(),
            w
        );
    }
}

/// every setting's default rendered value must fit the value column.
#[test]
fn setting_default_values_fit_the_value_column() {
    let config = Config::default();
    for setting in Setting::all() {
        let v = setting.current_value(&config);
        let w = UnicodeWidthStr::width(v.as_str());
        assert!(
            w <= 26,
            "{} value {:?} is {} cells wide (value column is 26)",
            setting.name(),
            v,
            w
        );
    }
}

/// every enum option must fit the value column.
#[test]
fn setting_enum_options_fit_the_value_column() {
    for setting in Setting::all() {
        if setting.is_enum() {
            for opt in setting.enum_options() {
                let w = UnicodeWidthStr::width(*opt);
                assert!(
                    w <= 26,
                    "{} option {:?} is {} cells wide",
                    setting.name(),
                    opt,
                    w
                );
            }
        }
    }
}

#[test]
fn settings_menu_tab_filtering_works() {
    // Verify that tabs filter settings correctly
    let quick_settings = SettingTab::Quick.filtered_settings();
    let visuals_settings = SettingTab::Visuals.filtered_settings();
    let layout_settings = SettingTab::Layout.filtered_settings();
    let advanced_settings = SettingTab::Advanced.filtered_settings();

    // Quick tab should have animation toggle
    assert!(
        quick_settings.contains(&Setting::AnimationEnabled),
        "Quick tab should contain AnimationEnabled"
    );

    // Visuals tab should have dim strength
    assert!(
        visuals_settings.contains(&Setting::DimStrength),
        "Visuals tab should contain DimStrength"
    );

    // Layout tab should have gap settings
    assert!(
        layout_settings.contains(&Setting::GapInner),
        "Layout tab should contain GapInner"
    );

    // Advanced tab should have shell
    assert!(
        advanced_settings.contains(&Setting::Shell),
        "Advanced tab should contain Shell"
    );

    // Tabs should be disjoint or have minimal overlap (except quick which is user-facing)
    assert!(!quick_settings.is_empty(), "Quick tab should not be empty");
    assert!(
        !visuals_settings.is_empty(),
        "Visuals tab should not be empty"
    );
    assert!(
        !layout_settings.is_empty(),
        "Layout tab should not be empty"
    );
    assert!(
        !advanced_settings.is_empty(),
        "Advanced tab should not be empty"
    );
}

#[test]
fn numeric_settings_support_arrow_increment() {
    // Verify that numeric settings are marked for arrow increment
    let numeric_settings = [
        Setting::LayoutDurationMs,
        Setting::FrameIntervalMs,
        Setting::ResizeStep,
        Setting::DimStrength,
        Setting::GapInner,
        Setting::GapOuter,
        Setting::MinPaneCols,
        Setting::MinPaneRows,
        Setting::SidebarWidth,
    ];

    for setting in numeric_settings {
        assert!(
            setting.supports_arrow_increment(),
            "numeric setting {} should support arrow increment",
            setting.name()
        );
    }
}

#[test]
fn setting_application_works() {
    let mut config = Config::default();

    // Test bool toggle
    let initial = Setting::AnimationEnabled.current_value(&config);
    Setting::AnimationEnabled
        .apply(&mut config, "false")
        .expect("should apply");
    let after = Setting::AnimationEnabled.current_value(&config);
    assert_ne!(initial, after, "animation enabled should toggle");

    // Test numeric value
    Setting::ResizeStep
        .apply(&mut config, "5")
        .expect("should apply");
    let value = Setting::ResizeStep.current_value(&config);
    assert_eq!(value, "5", "resize step should be 5");

    // Test enum value
    let theme_opts = Setting::ThemeName.enum_options();
    if !theme_opts.is_empty() {
        Setting::ThemeName
            .apply(&mut config, theme_opts[0])
            .expect("should apply");
        let value = Setting::ThemeName.current_value(&config);
        assert_eq!(value, theme_opts[0], "theme should be set");
    }
}

#[test]
fn settings_menu_state_defaults_to_quick_tab() {
    let state = SettingsMenuState::default();
    assert_eq!(
        state.active_tab,
        SettingTab::Quick,
        "should start on Quick tab"
    );
    assert_eq!(state.selected_idx, 0, "should start at index 0");
    assert!(state.filter.is_empty(), "should have empty filter");
}

#[test]
fn enum_cycling_wraps_around() {
    // Test that enum options cycle correctly
    let opts = Setting::ProfileMode.enum_options();
    assert!(!opts.is_empty(), "ProfileMode should have enum options");
    assert_eq!(opts.len(), 4, "ProfileMode should have 4 options");
    assert_eq!(opts[0], "beginner");
    assert_eq!(opts[1], "hypr");
    assert_eq!(opts[2], "vim");
    assert_eq!(opts[3], "dev");
}

#[test]
fn all_settings_have_current_value() {
    let config = Config::default();
    let all_settings = Setting::all();

    for setting in all_settings {
        let value = setting.current_value(&config);
        // Shell can be empty if not set in config, that's ok
        // Text settings should still return something without panicking
        if setting.name() != "shell" {
            assert!(
                !value.is_empty() || setting.is_enum(),
                "setting {} should have a current value, got: '{}'",
                setting.name(),
                value
            );
        }
    }
}

#[test]
fn setting_categories_are_defined() {
    let all_settings = Setting::all();

    for setting in all_settings {
        let category = setting.category();
        assert!(
            !category.is_empty(),
            "setting {} should have a category",
            setting.name()
        );
    }
}

#[test]
fn all_settings_have_names_and_descriptions() {
    let all_settings = Setting::all();

    for setting in all_settings {
        let name = setting.name();
        let desc = setting.description();
        assert!(!name.is_empty(), "setting should have a name");
        assert!(
            !desc.is_empty(),
            "setting {} should have a description",
            name
        );
    }
}

// ============================================================================
// COMPREHENSIVE REGRESSION TESTS: Space vs Enter Behavior
// ============================================================================

#[test]
fn space_toggles_bool_settings() {
    let mut config = Config::default();
    let bool_settings = [
        Setting::AnimationEnabled,
        Setting::DimInactive,
        Setting::Mouse,
        Setting::Pet,
        Setting::ShowWelcome,
        Setting::ShowHostname,
    ];

    for setting in bool_settings {
        let initial = setting.current_value(&config);
        let toggle_to = if initial == "true" { "false" } else { "true" };
        setting.apply(&mut config, toggle_to).expect("apply");
        let after = setting.current_value(&config);
        assert_ne!(
            initial,
            after,
            "space should toggle {} (was {}, now {})",
            setting.name(),
            initial,
            after
        );
    }
}

#[test]
fn space_cycles_enum_settings() {
    let mut config = Config::default();
    let enum_settings = [
        (Setting::ProfileMode, Setting::ProfileMode.enum_options()),
        (Setting::LayoutMode, Setting::LayoutMode.enum_options()),
        (Setting::BorderStyle, Setting::BorderStyle.enum_options()),
        (Setting::WorkspaceBar, Setting::WorkspaceBar.enum_options()),
        (
            Setting::SidebarDensity,
            Setting::SidebarDensity.enum_options(),
        ),
    ];

    for (setting, opts) in enum_settings {
        if !opts.is_empty() {
            let initial = setting.current_value(&config);
            let pos = opts.iter().position(|o| *o == initial).unwrap_or(0);
            let next = opts[(pos + 1) % opts.len()];
            setting.apply(&mut config, next).expect("apply");
            let after = setting.current_value(&config);
            assert_eq!(
                after,
                next,
                "space should cycle {} to next enum value",
                setting.name()
            );
        }
    }
}

#[test]
fn all_enum_settings_have_cyclic_options() {
    let all_settings = Setting::all();

    for setting in all_settings.iter().filter(|s| s.is_enum()) {
        let opts = setting.enum_options();
        assert!(
            !opts.is_empty(),
            "enum setting {} should have options",
            setting.name()
        );
        assert!(
            opts.len() >= 2,
            "enum setting {} should have at least 2 options, got {}",
            setting.name(),
            opts.len()
        );
    }
}

#[test]
fn bool_settings_can_be_toggled_multiple_times() {
    let mut config = Config::default();
    let setting = Setting::AnimationEnabled;

    for _ in 0..5 {
        let current = setting.current_value(&config);
        let next = if current == "true" { "false" } else { "true" };
        setting.apply(&mut config, next).expect("apply");
        let after = setting.current_value(&config);
        assert_eq!(after, next, "bool should toggle correctly each time");
    }
}

#[test]
fn enum_cycling_all_combinations_work() {
    // Test that we can cycle through all enum values without error
    let enum_settings = [
        Setting::ProfileMode,
        Setting::LayoutMode,
        Setting::SplitAxis,
        Setting::BorderStyle,
        Setting::LayoutEasing,
        Setting::WorkspaceBar,
        Setting::SidebarDensity,
    ];

    for setting in enum_settings {
        let mut config = Config::default();
        let opts = setting.enum_options();
        let len = opts.len();

        // Cycle through all options
        for _ in 0..len + 1 {
            let current = setting.current_value(&config);
            let pos = opts.iter().position(|o| *o == current).unwrap_or(0);
            let next = opts[(pos + 1) % opts.len()];
            let result = setting.apply(&mut config, next);
            assert!(
                result.is_ok(),
                "should be able to cycle {} to {}",
                setting.name(),
                next
            );
        }
    }
}

#[test]
fn numeric_validation_prevents_invalid_ranges() {
    let mut config = Config::default();

    // ResizeStep must be 1-100
    assert!(
        Setting::ResizeStep.apply(&mut config, "0").is_err(),
        "ResizeStep should reject 0"
    );
    assert!(
        Setting::ResizeStep.apply(&mut config, "101").is_err(),
        "ResizeStep should reject 101"
    );
    assert!(
        Setting::ResizeStep.apply(&mut config, "50").is_ok(),
        "ResizeStep should accept 50"
    );

    // MinPaneCols must be >= 6
    assert!(
        Setting::MinPaneCols.apply(&mut config, "5").is_err(),
        "MinPaneCols should reject 5"
    );
    assert!(
        Setting::MinPaneCols.apply(&mut config, "10").is_ok(),
        "MinPaneCols should accept 10"
    );

    // MinPaneRows must be >= 2
    assert!(
        Setting::MinPaneRows.apply(&mut config, "1").is_err(),
        "MinPaneRows should reject 1"
    );
    assert!(
        Setting::MinPaneRows.apply(&mut config, "3").is_ok(),
        "MinPaneRows should accept 3"
    );

    // DimStrength must be 0.0-1.0
    assert!(
        Setting::DimStrength.apply(&mut config, "-0.1").is_err(),
        "DimStrength should reject negative"
    );
    assert!(
        Setting::DimStrength.apply(&mut config, "1.5").is_err(),
        "DimStrength should reject >1.0"
    );
    assert!(
        Setting::DimStrength.apply(&mut config, "0.5").is_ok(),
        "DimStrength should accept 0.5"
    );
}

#[test]
fn weird_input_handling_edge_cases() {
    let mut config = Config::default();

    // Whitespace trimming
    let result = Setting::ResizeStep.apply(&mut config, "  50  ");
    assert!(result.is_ok(), "should trim whitespace from numeric input");

    // Invalid non-numeric input
    let result = Setting::Scrollback.apply(&mut config, "abc123");
    assert!(result.is_err(), "should reject non-numeric for scrollback");

    // Empty string is not a number
    let result = Setting::LayoutDurationMs.apply(&mut config, "");
    assert!(result.is_err(), "layout_duration_ms must be a number");

    // Very large numbers (should parse but might be clamped)
    let _result = Setting::MaxClients.apply(&mut config, "999999");
    // Should parse without panic, validation depends on config constraints

    // Special characters in text settings
    let result = Setting::Shell.apply(&mut config, "/bin/bash --norc --noprofile");
    assert!(result.is_ok(), "shell should accept special characters");
}

#[test]
fn all_tabs_contain_unique_settings_mostly() {
    // Each tab should have mostly unique settings, with minimal overlap
    let quick = SettingTab::Quick.filtered_settings();
    let visuals = SettingTab::Visuals.filtered_settings();
    let layout = SettingTab::Layout.filtered_settings();
    let advanced = SettingTab::Advanced.filtered_settings();

    // Count overlaps - there may be some intentional overlap (e.g., theme settings)
    let mut overlap_count = 0;
    for setting in quick.iter() {
        if visuals.contains(setting) || layout.contains(setting) || advanced.contains(setting) {
            overlap_count += 1;
        }
    }

    // Allow some overlap (< 50% of quick tab, since some visual/anim settings appear in multiple tabs)
    let overlap_ratio = overlap_count as f32 / quick.len() as f32;
    assert!(
        overlap_ratio < 0.5,
        "tabs should have reasonable overlap, got {:.1}%",
        overlap_ratio * 100.0
    );
}

#[test]
fn setting_kind_classification_is_consistent() {
    let all_settings = Setting::all();

    for setting in all_settings {
        let kind = setting.kind();
        match kind {
            venti::render::settings_menu::Kind::Bool => {
                assert!(
                    setting.is_bool(),
                    "{} is classified as Bool but is_bool() is false",
                    setting.name()
                );
            }
            venti::render::settings_menu::Kind::Enum => {
                assert!(
                    setting.is_enum(),
                    "{} is classified as Enum but is_enum() is false",
                    setting.name()
                );
                let opts = setting.enum_options();
                assert!(
                    !opts.is_empty(),
                    "enum setting {} should have options",
                    setting.name()
                );
            }
            venti::render::settings_menu::Kind::Int | venti::render::settings_menu::Kind::Float => {
                // Numeric - should not be bool or enum
                assert!(
                    !setting.is_bool(),
                    "numeric setting {} should not be bool",
                    setting.name()
                );
            }
            venti::render::settings_menu::Kind::Text => {
                // Text - open-ended
                assert!(
                    !setting.is_bool(),
                    "text setting {} should not be bool",
                    setting.name()
                );
            }
        }
    }
}

#[test]
fn applying_values_preserves_other_settings() {
    let mut config = Config::default();
    let animation_initial = Setting::AnimationEnabled.current_value(&config);
    let theme_initial = Setting::ThemeName.current_value(&config);

    // Change one setting
    Setting::ResizeStep.apply(&mut config, "15").expect("apply");

    // Other settings should be unchanged
    assert_eq!(
        Setting::AnimationEnabled.current_value(&config),
        animation_initial,
        "changing one setting should not affect others"
    );
    assert_eq!(
        Setting::ThemeName.current_value(&config),
        theme_initial,
        "changing one setting should not affect others"
    );
}

#[test]
fn rapid_toggle_bool_settings() {
    let mut config = Config::default();
    let settings = [
        Setting::AnimationEnabled,
        Setting::DimInactive,
        Setting::Mouse,
    ];

    // Rapid toggles should not panic or corrupt state
    for _ in 0..100 {
        for setting in settings {
            let current = setting.current_value(&config);
            let next = if current == "true" { "false" } else { "true" };
            let _ = setting.apply(&mut config, next);
        }
    }
}

#[test]
fn rapid_enum_cycling() {
    let mut config = Config::default();
    let settings = [
        Setting::ProfileMode,
        Setting::BorderStyle,
        Setting::WorkspaceBar,
    ];

    // Rapid cycles should not panic or corrupt state
    for _ in 0..100 {
        for setting in settings {
            let opts = setting.enum_options();
            let current = setting.current_value(&config);
            let pos = opts.iter().position(|o| *o == current).unwrap_or(0);
            let next = opts[(pos + 1) % opts.len()];
            let _ = setting.apply(&mut config, next);
        }
    }
}

#[test]
fn rapid_numeric_changes() {
    let mut config = Config::default();

    // Rapid numeric increments/decrements
    for i in 0..100 {
        let val = (i % 50 + 1).to_string(); // 1-50
        let _ = Setting::ResizeStep.apply(&mut config, &val);
    }
}

// ============================================================================
// DOCUMENTATION & COMPLETENESS CHECKS
// ============================================================================

#[test]
fn all_settings_documented_in_descriptions() {
    let all_settings = Setting::all();

    for setting in all_settings {
        let desc = setting.description();
        // Check for placeholder/bad descriptions
        assert!(
            !desc.contains("TODO"),
            "{} description should not contain TODO",
            setting.name()
        );
        assert!(
            !desc.contains("placeholder"),
            "{} description should not be placeholder",
            setting.name()
        );
        assert!(desc.len() > 3, "{} description too short", setting.name());
        assert!(
            desc.len() < 100,
            "{} description too long ({})",
            setting.name(),
            desc.len()
        );
    }
}

#[test]
fn kind_matches_apply_behavior() {
    // Verify that Kind enum matches actual apply() behavior
    let config = Config::default();

    // All Ints should parse as numbers
    for setting in Setting::all()
        .iter()
        .filter(|s| matches!(s.kind(), venti::render::settings_menu::Kind::Int))
    {
        let current = setting.current_value(&config);
        // Should be parseable as number (or empty for optional)
        if !current.is_empty() {
            assert!(
                current.parse::<i64>().is_ok(),
                "int setting {} has non-numeric value: {}",
                setting.name(),
                current
            );
        }
    }

    // All Floats should parse as floats
    for setting in Setting::all()
        .iter()
        .filter(|s| matches!(s.kind(), venti::render::settings_menu::Kind::Float))
    {
        let current = setting.current_value(&config);
        if !current.is_empty() {
            assert!(
                current.parse::<f32>().is_ok(),
                "float setting {} has non-numeric value: {}",
                setting.name(),
                current
            );
        }
    }
}

#[test]
fn search_filter_applies_correctly_on_top_of_tabs() {
    // Regression test: search filter must apply after tab filtering
    // (bug: space toggle didn't work on filtered results)
    let config = Config::default();

    // Get Quick tab settings
    let quick = SettingTab::Quick.filtered_settings();

    // Search for animation-related settings
    let search = "animation";
    let matching: Vec<_> = quick
        .iter()
        .filter(|s| {
            s.name().to_lowercase().contains(search)
                || s.description().to_lowercase().contains(search)
                || s.category().to_lowercase().contains(search)
        })
        .collect();

    // Should find animation-related settings
    assert!(
        !matching.is_empty(),
        "search for '{}' should find settings",
        search
    );

    // Each match should have a current value
    for setting in matching {
        let current = setting.current_value(&config);
        assert!(
            !current.is_empty() || setting.is_enum(),
            "filtered setting {} should have a value",
            setting.name()
        );
    }
}
