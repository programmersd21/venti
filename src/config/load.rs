use super::schema::Config;
use super::validate::{ConfigValidationError, validate_toml};
use std::borrow::Cow;
use std::fs;
use std::path::Path;

/// saves configuration to a toml file, preserving as much of the user's file
/// as possible.
///
/// only the scalar keys that changed between `previous` and `config` are
/// rewritten; everything else in the file (comments, key order, formatting)
/// is kept verbatim. when nothing changed the file is left untouched. if the
/// change cannot be expressed as scalar-line edits (arrays/tables changed, or
/// the patched text fails to parse), the file is rewritten from the serialized
/// config instead, which always yields a valid file but drops comments.
pub fn save_to_file(config: &Config, path: &Path, previous: &Config) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|e| format!("failed to create config directory: {}", e))?;
    }

    let existing = fs::read_to_string(path).ok();
    let patched = existing
        .as_deref()
        .and_then(|raw| patch_config_text(raw, previous, config))
        .and_then(|patched| load_from_str(&patched).map(|_| patched).ok());
    let render = patched.unwrap_or_else(|| {
        toml::to_string_pretty(config).expect("config always serializes to toml")
    });

    if Some(&render) == existing.as_ref() {
        return Ok(());
    }

    // write atomically: write to temp file, then rename
    let tmp_path = path.with_extension("toml.tmp");
    fs::write(&tmp_path, &render).map_err(|e| format!("failed to write config file: {}", e))?;

    fs::rename(&tmp_path, path).map_err(|e| {
        let _ = fs::remove_file(&tmp_path);
        format!("failed to save config file: {}", e)
    })?;

    Ok(())
}

/// escapes a string for a toml basic string literal.
fn escape_toml_basic(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 8);
    for ch in s.chars() {
        match ch {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            ch if (ch as u32) < 0x20 => out.push_str(&format!("\\u{:04X}", ch as u32)),
            ch => out.push(ch),
        }
    }
    out
}

/// renders a scalar toml value; returns None for arrays and tables.
fn render_toml_scalar(value: &toml::Value) -> Option<String> {
    match value {
        toml::Value::String(s) => Some(format!("\"{}\"", escape_toml_basic(s))),
        toml::Value::Integer(i) => Some(i.to_string()),
        toml::Value::Float(f) => Some(format!("{f:?}")),
        toml::Value::Boolean(b) => Some(b.to_string()),
        toml::Value::Datetime(dt) => Some(dt.to_string()),
        toml::Value::Array(_) | toml::Value::Table(_) => None,
    }
}

/// collects scalar leaves that changed between two configs, as `section.key`.
fn collect_scalar_changes(
    old: &toml::Value,
    new: &toml::Value,
    section: &str,
    out: &mut Vec<(String, toml::Value)>,
) {
    let (toml::Value::Table(old_table), toml::Value::Table(new_table)) = (old, new) else {
        return;
    };
    for (key, new_value) in new_table {
        match old_table.get(key) {
            Some(old_value) => match (old_value, new_value) {
                (toml::Value::Table(_), toml::Value::Table(_)) => {
                    collect_scalar_changes(old_value, new_value, section, out)
                }
                // arrays/tables are only patched when fully unchanged
                (o, n) if o == n => {}
                // arrays/tables that changed fall back to a full rewrite
                (toml::Value::Array(_) | toml::Value::Table(_), _)
                | (_, toml::Value::Array(_) | toml::Value::Table(_)) => out.push((
                    format!("{section}.{key}"),
                    toml::Value::Table(Default::default()),
                )),
                (o, n) if o != n => out.push((format!("{section}.{key}"), n.clone())),
                _ => {}
            },
            None => out.push((format!("{section}.{key}"), new_value.clone())),
        }
    }
}

/// patches `raw` (the current file text) with the scalar diff between
/// `previous` and `config`. returns None when the change cannot be expressed
/// as line edits.
fn patch_config_text(raw: &str, previous: &Config, config: &Config) -> Option<String> {
    let old_value: toml::Value = toml::Value::try_from(previous).ok()?;
    let new_value: toml::Value = toml::Value::try_from(config).ok()?;

    let mut changes: Vec<(String, toml::Value)> = Vec::new();
    if let (toml::Value::Table(old), toml::Value::Table(new)) = (&old_value, &new_value) {
        for (section, new_section) in new {
            // sections containing array values are not patched in place
            let old_section = old.get(section)?;
            collect_scalar_changes(old_section, new_section, section, &mut changes);
        }
    }
    if changes.is_empty() {
        return Some(raw.to_string());
    }
    // any sentinel change means a rewrite is required
    if changes
        .iter()
        .any(|(_, v)| matches!(v, toml::Value::Table(t) if t.is_empty()))
    {
        return None;
    }

    let mut by_section: std::collections::HashMap<&str, Vec<(&str, String)>> =
        std::collections::HashMap::new();
    for (path, value) in &changes {
        let (section, key) = path.split_once('.')?;
        let rendered = render_toml_scalar(value)?;
        by_section.entry(section).or_default().push((key, rendered));
    }

    let mut out: Vec<String> = Vec::with_capacity(raw.lines().count() + 16);
    let mut current_section: Option<String> = None;
    let mut remaining: std::collections::HashMap<String, Vec<(String, String)>> = by_section
        .into_iter()
        .map(|(section, keys)| {
            (
                section.to_string(),
                keys.into_iter().map(|(k, r)| (k.to_string(), r)).collect(),
            )
        })
        .collect();
    let flush_section = |out: &mut Vec<String>, section: &str, rem: &mut Vec<(String, String)>| {
        if section == "binds" {
            // quoted keys here are not safely patched; keep them verbatim
            return;
        }
        for (key, rendered) in rem.drain(..) {
            out.push(format!("{key} = {rendered}"));
        }
    };

    for line in raw.lines() {
        let trimmed = line.trim();
        if let Some(end) = trimmed.strip_prefix('[').and_then(|r| r.find(']')) {
            // leaving the previous section: insert any keys it was missing
            if let Some(section) = current_section.take()
                && let Some(rem) = remaining.get_mut(section.as_str())
            {
                flush_section(&mut out, &section, rem);
            }
            current_section = Some(trimmed[1..=end].trim().to_string());
            out.push(line.to_string());
            continue;
        }
        let key_line = current_section.as_deref().and_then(|section| {
            remaining.get_mut(section).and_then(|keys| {
                keys.iter()
                    .position(|(key, _)| line_matches_key(line, key))
                    .map(|idx| keys.remove(idx))
            })
        });
        match key_line {
            Some((key, rendered)) => out.push(format!("{key} = {rendered}")),
            None => out.push(line.to_string()),
        }
    }
    if let Some(section) = current_section.take()
        && let Some(rem) = remaining.get_mut(section.as_str())
    {
        flush_section(&mut out, &section, rem);
    }

    // sections whose keys never got patched: insert them at the end of the file
    let mut tail: Vec<String> = Vec::new();
    for (section, keys) in remaining.drain() {
        if keys.is_empty() || section == "binds" {
            continue;
        }
        if !out.iter().any(|l| l.trim() == format!("[{section}]")) {
            tail.push(format!("[{section}]"));
        }
        for (key, rendered) in keys {
            tail.push(format!("{key} = {rendered}"));
        }
    }

    let mut text = out.join("\n");
    if raw.ends_with('\n') || !tail.is_empty() {
        text.push('\n');
    }
    text.push_str(&tail.join("\n"));
    Some(text)
}

/// true when `line` is a `key = value` assignment for `key` (quoted or bare).
fn line_matches_key(line: &str, key: &str) -> bool {
    let mut rest = line.trim_start();
    let mut quoted = false;
    if rest.starts_with('"') {
        quoted = true;
        rest = &rest[1..];
    } else if let Some(stripped) = rest.strip_prefix('"') {
        rest = stripped;
    }
    let key_end = rest.find(|c: char| {
        if quoted {
            c == '"'
        } else {
            c == '=' || c.is_whitespace()
        }
    });
    let Some(key_end) = key_end else {
        return false;
    };
    let name = &rest[..key_end];
    if quoted {
        // consume the closing quote and any spaces before '='
        rest = &rest[key_end + 1..];
        rest = rest.trim_start();
        name == key && rest.starts_with('=')
    } else {
        name == key && rest[key_end..].trim_start().starts_with('=')
    }
}

/// rewrites legacy key names from before the schema was pruned so an old
/// `[animation] duration_ms/easing` config still validates today. keys that
/// cannot be migrated are kept and reported by the validator instead.
fn migrate_legacy_toml(s: &str) -> Cow<'_, str> {
    let Ok(value) = s.parse::<toml::Value>() else {
        return Cow::Borrowed(s);
    };
    let toml::Value::Table(mut table) = value else {
        return Cow::Borrowed(s);
    };
    let mut changed = false;

    if let Some(toml::Value::Table(anim)) = table.get_mut("animation") {
        if let Some(v) = anim.remove("duration_ms") {
            anim.entry("layout_duration_ms".to_string()).or_insert(v);
            changed = true;
        }
        if let Some(v) = anim.remove("easing") {
            anim.entry("layout_easing".to_string()).or_insert(v);
            changed = true;
        }
        // dropped from the schema; pane layout animation covers workspace switches.
        if anim.remove("workspace_duration_ms").is_some() {
            changed = true;
        }
        if anim.remove("workspace_easing").is_some() {
            changed = true;
        }
    }

    if let Some(toml::Value::Table(general)) = table.get_mut("general")
        && general.remove("graphics_unicode_placeholders").is_some()
    {
        changed = true;
    }

    if !changed {
        return Cow::Borrowed(s);
    }
    match toml::to_string_pretty(&toml::Value::Table(table)) {
        Ok(out) => Cow::Owned(out),
        Err(_) => Cow::Borrowed(s),
    }
}

/// loads configuration from a string, returning parsed config or errors.
///
/// an empty file is NOT a valid config: toml parses it into an empty table,
/// which deserializes to `Config::default()`, so a transient empty read (editor
/// truncation, `> file`) would silently reset every setting to defaults, with no
/// error and no toast. treat it as an error so the hot reload keeps the last good
/// config and startup fails loudly instead.
pub fn load_from_str(s: &str) -> Result<Config, Vec<ConfigValidationError>> {
    let migrated;
    let s = match migrate_legacy_toml(s) {
        Cow::Owned(owned) => {
            migrated = owned;
            migrated.as_str()
        }
        Cow::Borrowed(borrowed) => borrowed,
    };
    if s.trim().is_empty() {
        return Err(vec![ConfigValidationError {
            line: None,
            message: "config file is empty".to_string(),
        }]);
    }
    let config: Config = match toml::from_str(s) {
        Ok(c) => c,
        Err(e) => {
            let line = e.span().map(|span| {
                // toml spans are byte offsets; clamp to a char boundary before slicing.
                let start = span.start.min(s.len());
                let boundary = (0..=start)
                    .rev()
                    .find(|&i| s.is_char_boundary(i))
                    .unwrap_or(0);
                s[..boundary].bytes().filter(|&b| b == b'\n').count() + 1
            });
            return Err(vec![ConfigValidationError {
                line,
                message: e.message().to_string(),
            }]);
        }
    };

    let errors = validate_toml(s, &config);
    if !errors.is_empty() {
        return Err(errors);
    }

    Ok(config)
}

/// loads configuration from a file path, falling back to default if file is absent.
pub fn load_from_file_or_default(path: &Path) -> Result<Config, Vec<ConfigValidationError>> {
    if !path.exists() {
        return Ok(Config::default());
    }

    let contents = fs::read_to_string(path).map_err(|e| {
        vec![ConfigValidationError {
            line: None,
            message: format!("failed to read config file {}: {}", path.display(), e),
        }]
    })?;

    load_from_str(&contents)
}

/// generates a fully commented default configuration file.
pub fn default_config_commented() -> &'static str {
    r##"# venti configuration file

[general]
# default shell program to run in panes; empty string uses $SHELL, then /bin/sh
shell = ""
# maximum scrollback history lines per pane
scrollback = 10000
# whether to synchronize terminal copy events via osc 52 escape sequences
osc52 = true
# forward sixel/kitty graphics from panes to clients.
# terminal graphics support is experimental: an image can land in the wrong
# place or leave pixels behind after a repaint. off by default; set true to
# try it, and false again if images misbehave.
graphics = true
# default number of initial workspaces to create
default_workspaces = 1
# friendly display names for default numbered workspaces
workspace_names = ["ws1"]
# whether to confirm before closing a pane with active foreground child processes
confirm_close_running = true
# maximum simultaneous client connections per server
max_clients = 32
# maximum number of panes per session
max_panes = 1000
# maximum number of workspaces per session
max_workspaces = 100

[profile]
# active keybinding profile: "beginner", "hypr", "vim" or "dev"
mode = "beginner"

[input]
# copy selected text immediately to clipboard when releasing mouse
copy_on_select = true
# enable mouse interaction
mouse = true
# bindings to pass directly to active terminal application
passthrough = []
# number of lines to scroll per mouse wheel tick
scroll_lines = 3
# key bindings the profile should not bind, e.g. ["alt+n"] frees them for the app
disabled_keys = []

[layout]
# workspace layout mode: "dwindle" or "monocle"
mode = "dwindle"
# automatic split direction policy: "auto", "right", or "down"
split_axis = "right"
# inner gap between adjacent panes in character cells
gap_inner = 0
# outer gap between workspace border and panes in character cells
gap_outer = 1
# percentage step size when resizing panes
resize_step = 5
# minimum allowable content width in character columns
min_pane_cols = 6
# minimum allowable content height in character rows
min_pane_rows = 2

[decorations]
# border style: "plain", "rounded", "thick", "double", or "none"
border_style = "rounded"
# hide pane borders when only a single pane occupies the workspace
hide_border_when_single = false
# dim inactive panes to emphasize focus
dim_inactive = true
# dim strength blend factor between 0.0 (no dimming) and 1.0 (full background)
dim_strength = 0.18
# render title bar in pane border
title = true
# show a small pet emoji next to the pane title of the focused pane
pet = true
# emoji used by the pet
pet_emoji = "🐱"
# show a scroll counter ("↑12") on the right of the pane title while the
# pane is scrolled back from the live bottom
scroll_indicator = false

[animation]
# enable pane layout change animations (swaps, splits, resizes)
enabled = true
# pane layout change animation duration in milliseconds
layout_duration_ms = 320
# pane layout change animation easing: "linear", "ease_out_cubic", "ease_in_out", or "ease_out_quint"
layout_easing = "ease_out_cubic"

[theme]
# theme name: any built-in theme. press f12 to browse them.
name = "mocha"
# optional color overrides on top of the named theme (hex, #rrggbb)
# base = "#1e1e2e"
# mantle = "#181825"
# surface = "#313244"
# overlay = "#45475a"
# text = "#cdd6f4"
# subtext = "#a6adc8"
# muted = "#9399b2"
# accent = "#89b4fa"
# info = "#94e2d5"
# success = "#a6e3a1"
# warn = "#f9e2af"
# alert = "#f38ba8"

[ui]
# position of workspace bar: "top", "bottom", "left", "right", or "off".
# left/right render a vertical workspace tab list with focused-pane subtitles.
workspace_bar = "top"
# width of the expanded vertical sidebar in character columns (left/right)
sidebar_width = 28
# width of the collapsed icon rail in character columns (toggle_sidebar / alt+s)
sidebar_collapsed_width = 4
# render the session name header at the top of the sidebar
sidebar_header = true
# start with the sidebar collapsed to the icon rail
sidebar_collapsed = false
# sidebar row density: "compact" (one row per tab) or "comfortable" (two rows: tab + focused pane title)
sidebar_density = "comfortable"
# render bottom context hint bar
hint_bar = true
# render clock in status bar
clock = true
# strftime format for status bar clock
clock_format = "%H:%M"
# restrict glyphs to standard 7-bit ascii
ascii = false
# render the workspace tab bar
show_tab_bar = true
# show the hostname in the status bar
show_hostname = true
# show the current time in the pane title
show_time_in_title = true
# show the current directory of the focused pane
show_pwd = true
# display welcome overlay on initial session startup
show_welcome = true
# target frame interval in milliseconds (e.g. 8 ms for ~125 fps cap)
frame_interval_ms = 8

[binds]
# custom keybindings overriding profile defaults
# "alt+shift+q" = "quit"
# "alt+g" = "workspace_goto 3"
"##
}

#[cfg(test)]
mod patch_tests {
    use super::*;

    fn default_toml() -> String {
        default_config_commented().to_string()
    }

    #[test]
    fn documented_template_covers_every_schema_key() {
        let template = default_toml();
        let default = toml::Value::try_from(Config::default()).expect("schema serializes");
        for (section, table) in default.as_table().expect("root table") {
            if section == "binds" {
                continue;
            }
            for key in table.as_table().expect("section table").keys() {
                let line = format!("{key} =");
                assert!(
                    template.contains(&line),
                    "the documented template is missing [{section}] {key}"
                );
            }
        }
    }

    #[test]
    fn patch_preserves_comments_and_rewrites_only_changed_scalar() {
        let raw = default_toml();
        let previous = Config::default();
        let mut changed = Config::default();
        changed.theme.name = "nord".to_string();
        let out = patch_config_text(&raw, &previous, &changed).expect("patchable");
        assert!(out.contains('#'), "comments must be preserved");
        assert!(out.contains("name = \"nord\""), "theme changed");
        assert_eq!(load_from_str(&out).unwrap().theme.name, "nord");
        // every other line is verbatim
        let old_lines: Vec<&str> = raw.lines().collect();
        let new_lines: Vec<&str> = out.lines().collect();
        let mut diff = 0;
        for (a, b) in old_lines.iter().zip(&new_lines) {
            if a != b {
                diff += 1;
                assert_eq!(a.trim().split('=').next().map(str::trim), Some("name"));
            }
        }
        assert_eq!(diff, 1);
    }

    #[test]
    fn patch_inserts_missing_scalar_into_existing_section() {
        // the documented template has no show_tab_bar key; toggling it must
        // not fall back to a full rewrite
        let raw = default_toml();
        let previous = Config::default();
        let mut changed = Config::default();
        changed.ui.show_tab_bar = !Config::default().ui.show_tab_bar;
        let out = patch_config_text(&raw, &previous, &changed).expect("patchable");
        assert!(out.contains("show_tab_bar = false"));
        assert!(!load_from_str(&out).unwrap().ui.show_tab_bar);
    }

    #[test]
    fn patch_no_changes_returns_identical_text() {
        let raw = default_toml();
        let config = Config::default();
        let out = patch_config_text(&raw, &config, &config).unwrap();
        assert_eq!(out, raw);
    }

    #[test]
    fn patch_array_change_returns_none_so_caller_falls_back() {
        let raw = default_toml();
        let previous = Config::default();
        let mut changed = Config::default();
        changed.general.workspace_names = vec!["a".to_string()];
        assert!(patch_config_text(&raw, &previous, &changed).is_none());
    }

    #[test]
    fn patched_output_always_parses_or_falls_back() {
        // weird-but-valid user file: section with a trailing comment header,
        // keys with odd spacing; must patch or fall back, never corrupt
        let raw = "[ui] # panel\nclock = true  \n\n[theme]\nname = \"mocha\"\n";
        let previous = Config::default();
        let mut changed = Config::default();
        changed.ui.clock = false;
        changed.theme.name = "nord".to_string();
        if let Some(out) = patch_config_text(raw, &previous, &changed) {
            let parsed = load_from_str(&out).unwrap();
            assert!(!parsed.ui.clock);
            assert_eq!(parsed.theme.name, "nord");
        }
    }
}
