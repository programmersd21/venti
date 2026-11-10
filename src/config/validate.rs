use std::fmt;

/// root table of the schema's default config: every section and key the
/// deserializer produces is by definition known, so deriving the lists from
/// it can never drift out of sync with the schema again.
fn schema_table() -> toml::Value {
    toml::Value::try_from(super::schema::Config::default())
        .unwrap_or(toml::Value::Table(toml::map::Map::new()))
}

/// known top-level sections, derived from the schema default.
fn known_sections(table: &toml::Value) -> Vec<String> {
    table
        .as_table()
        .map(|t| t.keys().cloned().collect())
        .unwrap_or_default()
}

/// known keys for a section, derived from the schema default.
fn known_section_keys(table: &toml::Value, section: &str) -> Vec<String> {
    table
        .get(section)
        .and_then(|v| v.as_table())
        .map(|t| t.keys().cloned().collect())
        .unwrap_or_default()
}

/// known command names valid for keybinding.
pub const KNOWN_COMMANDS: &[&str] = &[
    "new_pane",
    "split_right",
    "split_down",
    "close_pane",
    "focus_left",
    "focus_down",
    "focus_up",
    "focus_right",
    "focus_next",
    "focus_prev",
    "swap_left",
    "swap_down",
    "swap_up",
    "swap_right",
    "resize_left",
    "resize_down",
    "resize_up",
    "resize_right",
    "resize_mode",
    "equalize",
    "rotate_split",
    "toggle_zoom",
    "toggle_sidebar",
    "toggle_float",
    "float_center",
    "workspace_goto",
    "workspace_next",
    "workspace_prev",
    "new_workspace",
    "close_workspace",
    "move_to_workspace",
    "move_to_workspace_next",
    "move_to_workspace_prev",
    "rename_workspace",
    "toggle_scratch",
    "copy_mode",
    "search",
    "paste_buffer",
    "command_palette",
    "theme_menu",
    "settings_menu",
    "help",
    "reload_config",
    "detach",
    "kill_session",
    "show_welcome",
    "cat",
];

/// detailed config validation error.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConfigValidationError {
    pub line: Option<usize>,
    pub message: String,
}

impl fmt::Display for ConfigValidationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(l) = self.line {
            write!(f, "config error at line {}: {}", l, self.message)
        } else {
            write!(f, "config error: {}", self.message)
        }
    }
}

/// validates raw toml text and values against venti schema rules.
pub fn validate_toml(raw_toml: &str, config: &super::schema::Config) -> Vec<ConfigValidationError> {
    let mut errors = Vec::new();

    let table = schema_table();
    let sections = known_sections(&table);

    if let Ok(toml::Value::Table(root_map)) = raw_toml.parse::<toml::Value>() {
        for (key, val) in root_map {
            if !sections.contains(&key) {
                let suggestion = crate::util::fuzzy::suggest_candidate(
                    &key,
                    &sections.iter().map(String::as_str).collect::<Vec<_>>(),
                );
                let msg = if let Some(s) = suggestion {
                    format!("unknown section \"{key}\", did you mean \"{s}\"?")
                } else {
                    format!("unknown section \"{key}\"")
                };
                errors.push(ConfigValidationError {
                    line: find_line_number(raw_toml, &key),
                    message: msg,
                });
                continue;
            }

            // if it is a table, check section keys (except binds which are arbitrary key names)
            if key != "binds"
                && let toml::Value::Table(section_map) = val
            {
                let allowed_keys = known_section_keys(&table, &key);
                let allowed_refs: Vec<&str> = allowed_keys.iter().map(String::as_str).collect();

                for subkey in section_map.keys() {
                    if !allowed_refs.contains(&subkey.as_str()) {
                        let suggestion =
                            crate::util::fuzzy::suggest_candidate(subkey, &allowed_refs);
                        let msg = if let Some(s) = suggestion {
                            format!("unknown key \"{subkey}\", did you mean \"{s}\"?")
                        } else {
                            format!("unknown key \"{subkey}\"")
                        };
                        errors.push(ConfigValidationError {
                            line: find_line_number(raw_toml, subkey),
                            message: msg,
                        });
                    }
                }
            }
        }
    }

    if !(0.0..=1.0).contains(&config.decorations.dim_strength) {
        errors.push(ConfigValidationError {
            line: find_line_number(raw_toml, "dim_strength"),
            message: format!(
                "dim_strength must be between 0.0 and 1.0, found {}",
                config.decorations.dim_strength
            ),
        });
    }

    if !(4..=100).contains(&config.ui.frame_interval_ms) {
        errors.push(ConfigValidationError {
            line: find_line_number(raw_toml, "frame_interval_ms"),
            message: format!(
                "frame_interval_ms must be between 4 and 100, found {}",
                config.ui.frame_interval_ms
            ),
        });
    }

    if config.layout.min_pane_cols < 6 {
        errors.push(ConfigValidationError {
            line: find_line_number(raw_toml, "min_pane_cols"),
            message: format!(
                "min_pane_cols must be at least 6, found {}",
                config.layout.min_pane_cols
            ),
        });
    }

    if config.layout.min_pane_rows < 2 {
        errors.push(ConfigValidationError {
            line: find_line_number(raw_toml, "min_pane_rows"),
            message: format!(
                "min_pane_rows must be at least 2, found {}",
                config.layout.min_pane_rows
            ),
        });
    }

    // validate binds: key must parse and command must be known
    for (key, cmd_str) in &config.binds {
        if let Err(e) = crate::input::parse_key(key) {
            errors.push(ConfigValidationError {
                line: find_line_number(raw_toml, key),
                message: format!("invalid bind key \"{key}\": {e}"),
            });
        }

        let cmd_name = cmd_str.split_whitespace().next().unwrap_or("");
        if !crate::input::commands::Command::is_known_name(cmd_name) {
            let suggestion = crate::util::fuzzy::suggest_candidate(cmd_name, KNOWN_COMMANDS);
            let msg = if let Some(s) = suggestion {
                format!("unknown command \"{cmd_name}\" bound to \"{key}\", did you mean \"{s}\"?")
            } else {
                format!("unknown command \"{cmd_name}\" bound to \"{key}\"")
            };
            errors.push(ConfigValidationError {
                line: find_line_number(raw_toml, key),
                message: msg,
            });
        }
    }

    if !(1..=100).contains(&config.layout.resize_step) {
        errors.push(ConfigValidationError {
            line: find_line_number(raw_toml, "resize_step"),
            message: format!(
                "resize_step must be between 1 and 100, found {}",
                config.layout.resize_step
            ),
        });
    }

    // theme name must exist (or be a builtin we can resolve)
    if !config.theme.name.is_empty()
        && crate::config::theme::Theme::builtin(&config.theme.name).is_none()
    {
        errors.push(ConfigValidationError {
            line: find_line_number(raw_toml, "name"),
            message: format!(
                "unknown theme \"{}\"; see docs/configuration.md for the list of builtin themes",
                config.theme.name
            ),
        });
    }

    errors
}

/// finds the 1-based line number in text containing the token.
fn find_line_number(text: &str, token: &str) -> Option<usize> {
    for (idx, line) in text.lines().enumerate() {
        if line.contains(token) {
            return Some(idx + 1);
        }
    }
    None
}
