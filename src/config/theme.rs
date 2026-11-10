use ratatui::style::Color;
use serde::{Deserialize, Serialize};

/// 24-bit rgb color representation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct RgbColor {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

impl RgbColor {
    /// constructs an rgb color from individual channel bytes.
    pub const fn new(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b }
    }

    /// converts to a ratatui color.
    pub const fn to_ratatui(self) -> Color {
        Color::Rgb(self.r, self.g, self.b)
    }

    /// parses a 7-character hex string (#rrggbb).
    pub fn parse_hex(hex: &str) -> Result<Self, String> {
        let trimmed = hex.trim();
        if !trimmed.starts_with('#') || trimmed.len() != 7 {
            return Err(format!("invalid hex color format: {hex}"));
        }
        let r = u8::from_str_radix(&trimmed[1..3], 16)
            .map_err(|e| format!("invalid red channel in {hex}: {e}"))?;
        let g = u8::from_str_radix(&trimmed[3..5], 16)
            .map_err(|e| format!("invalid green channel in {hex}: {e}"))?;
        let b = u8::from_str_radix(&trimmed[5..7], 16)
            .map_err(|e| format!("invalid blue channel in {hex}: {e}"))?;
        Ok(Self { r, g, b })
    }

    /// blends this color toward a target color using linear srgb interpolation.
    pub fn blend(self, target: Self, factor: f32) -> Self {
        let t = factor.clamp(0.0, 1.0);
        let r = (self.r as f32 + (target.r as f32 - self.r as f32) * t).round() as u8;
        let g = (self.g as f32 + (target.g as f32 - self.g as f32) * t).round() as u8;
        let b = (self.b as f32 + (target.b as f32 - self.b as f32) * t).round() as u8;
        Self { r, g, b }
    }

    /// quantizes an rgb color to an xterm 256-color index.
    pub fn quantize_256(self) -> u8 {
        if self.r == self.g && self.g == self.b {
            if self.r < 8 {
                return 16;
            }
            if self.r > 248 {
                return 231;
            }
            let grey = (((self.r as u16 - 8) * 24 + 120) / 240) as u8;
            return 232 + grey.min(23);
        }

        let r_idx = if self.r < 48 {
            0
        } else if self.r < 115 {
            1
        } else {
            (self.r - 35) / 40
        };
        let g_idx = if self.g < 48 {
            0
        } else if self.g < 115 {
            1
        } else {
            (self.g - 35) / 40
        };
        let b_idx = if self.b < 48 {
            0
        } else if self.b < 115 {
            1
        } else {
            (self.b - 35) / 40
        };

        16 + 36 * r_idx.min(5) + 6 * g_idx.min(5) + b_idx.min(5)
    }
}

/// raw theme definition holding hex strings as read from toml.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RawTheme {
    pub name: String,
    pub base: String,
    pub mantle: String,
    pub surface: String,
    pub overlay: String,
    pub text: String,
    pub subtext: String,
    pub muted: String,
    pub accent: String,
    pub info: String,
    pub success: String,
    pub warn: String,
    pub alert: String,
}

/// resolved theme with pre-parsed rgb colors.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Theme {
    pub name: String,
    pub base: RgbColor,
    pub mantle: RgbColor,
    pub surface: RgbColor,
    pub overlay: RgbColor,
    pub text: RgbColor,
    pub subtext: RgbColor,
    pub muted: RgbColor,
    pub accent: RgbColor,
    pub info: RgbColor,
    pub success: RgbColor,
    pub warn: RgbColor,
    pub alert: RgbColor,
}

/// names of all embedded themes, in picker order.
pub const BUILTIN_THEMES: &[&str] = &[
    "mocha",
    "tokyo-night",
    "gruvbox-dark",
    "dracula",
    "nord",
    "catppuccin-frappe",
    "catppuccin-macchiato",
    "everforest",
    "kanagawa",
    "rose-pine",
    "rose-pine-moon",
    "solarized-dark",
    "monokai",
    "one-dark",
    "vesper",
];

impl Theme {
    /// resolves raw hex strings into an rgb theme.
    pub fn from_raw(raw: &RawTheme) -> Result<Self, String> {
        Ok(Self {
            name: raw.name.clone(),
            base: RgbColor::parse_hex(&raw.base)?,
            mantle: RgbColor::parse_hex(&raw.mantle)?,
            surface: RgbColor::parse_hex(&raw.surface)?,
            overlay: RgbColor::parse_hex(&raw.overlay)?,
            text: RgbColor::parse_hex(&raw.text)?,
            subtext: RgbColor::parse_hex(&raw.subtext)?,
            muted: RgbColor::parse_hex(&raw.muted)?,
            accent: RgbColor::parse_hex(&raw.accent)?,
            info: RgbColor::parse_hex(&raw.info)?,
            success: RgbColor::parse_hex(&raw.success)?,
            warn: RgbColor::parse_hex(&raw.warn)?,
            alert: RgbColor::parse_hex(&raw.alert)?,
        })
    }

    /// applies the color overrides from the config on top of a built-in theme.
    /// an override that is not a valid hex color is reported, the theme keeps
    /// the built-in color for that slot.
    pub fn apply_overrides(
        &mut self,
        config: &crate::config::schema::ThemeConfig,
    ) -> Result<(), String> {
        type Slot<'a> = (
            &'static str,
            Option<&'a String>,
            fn(&mut Theme) -> &mut RgbColor,
        );
        let slots: [Slot<'_>; 12] = [
            ("base", config.base.as_ref(), |t| &mut t.base),
            ("mantle", config.mantle.as_ref(), |t| &mut t.mantle),
            ("surface", config.surface.as_ref(), |t| &mut t.surface),
            ("overlay", config.overlay.as_ref(), |t| &mut t.overlay),
            ("text", config.text.as_ref(), |t| &mut t.text),
            ("subtext", config.subtext.as_ref(), |t| &mut t.subtext),
            ("muted", config.muted.as_ref(), |t| &mut t.muted),
            ("accent", config.accent.as_ref(), |t| &mut t.accent),
            ("info", config.info.as_ref(), |t| &mut t.info),
            ("success", config.success.as_ref(), |t| &mut t.success),
            ("warn", config.warn.as_ref(), |t| &mut t.warn),
            ("alert", config.alert.as_ref(), |t| &mut t.alert),
        ];
        for (name, value, slot) in &slots {
            let Some(value) = value else { continue };
            *slot(self) = RgbColor::parse_hex(value).map_err(|e| format!("{name}: {e}"))?;
        }
        Ok(())
    }

    /// resolves the configured theme: the named built-in theme with the
    /// configured color overrides applied.
    pub fn resolve(config: &crate::config::schema::ThemeConfig) -> Self {
        let mut theme = Self::builtin(&config.name).unwrap_or_else(Self::default_theme);
        if let Err(e) = theme.apply_overrides(config) {
            tracing::warn!("invalid theme color ignored: {e}");
        }
        theme
    }

    /// retrieves an embedded built-in theme by name. parsed once per process;
    /// the menu calls this per row per frame.
    pub fn builtin(name: &str) -> Option<Self> {
        static BUILTINS: std::sync::OnceLock<Vec<Theme>> = std::sync::OnceLock::new();
        BUILTINS
            .get_or_init(|| {
                BUILTIN_THEMES
                    .iter()
                    .filter_map(|name| Self::parse_builtin(name))
                    .collect()
            })
            .iter()
            .find(|t| t.name == name)
            .cloned()
    }

    fn parse_builtin(name: &str) -> Option<Self> {
        let raw_str = match name {
            "mocha" => include_str!("../../themes/mocha.toml"),
            "tokyo-night" => include_str!("../../themes/tokyo-night.toml"),
            "gruvbox-dark" => include_str!("../../themes/gruvbox-dark.toml"),
            "dracula" => include_str!("../../themes/dracula.toml"),
            "nord" => include_str!("../../themes/nord.toml"),
            "everforest" => include_str!("../../themes/everforest.toml"),
            "catppuccin-frappe" => include_str!("../../themes/catppuccin-frappe.toml"),
            "catppuccin-macchiato" => {
                include_str!("../../themes/catppuccin-macchiato.toml")
            }
            "kanagawa" => include_str!("../../themes/kanagawa.toml"),
            "rose-pine" => include_str!("../../themes/rose-pine.toml"),
            "rose-pine-moon" => include_str!("../../themes/rose-pine-moon.toml"),
            "solarized-dark" => include_str!("../../themes/solarized-dark.toml"),
            "monokai" => include_str!("../../themes/monokai.toml"),
            "one-dark" => include_str!("../../themes/one-dark.toml"),
            "vesper" => include_str!("../../themes/vesper.toml"),
            _ => return None,
        };
        let raw: RawTheme = toml::from_str(raw_str).ok()?;
        Self::from_raw(&raw).ok()
    }

    /// returns true when the theme background is dark.
    pub fn is_dark(&self) -> bool {
        let luminance =
            0.299 * self.base.r as f32 + 0.587 * self.base.g as f32 + 0.114 * self.base.b as f32;
        luminance < 128.0
    }

    /// default built-in theme (mocha).
    pub fn default_theme() -> Self {
        Self::builtin("mocha").expect("embedded mocha theme is valid")
    }

    /// color accessors as ratatui colors.
    pub fn base_color(&self) -> Color {
        self.base.to_ratatui()
    }
    pub fn surface_color(&self) -> Color {
        self.surface.to_ratatui()
    }
    pub fn overlay_color(&self) -> Color {
        self.overlay.to_ratatui()
    }
    pub fn text_color(&self) -> Color {
        self.text.to_ratatui()
    }
    pub fn subtext_color(&self) -> Color {
        self.subtext.to_ratatui()
    }
    pub fn muted_color(&self) -> Color {
        self.muted.to_ratatui()
    }
    pub fn accent_color(&self) -> Color {
        self.accent.to_ratatui()
    }
    pub fn info_color(&self) -> Color {
        self.info.to_ratatui()
    }
    pub fn success_color(&self) -> Color {
        self.success.to_ratatui()
    }
    pub fn warn_color(&self) -> Color {
        self.warn.to_ratatui()
    }
    pub fn alert_color(&self) -> Color {
        self.alert.to_ratatui()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::schema::ThemeConfig;

    #[test]
    fn overrides_change_only_the_slots_they_name() {
        let mut theme = Theme::builtin("mocha").expect("mocha ships with venti");
        let base = theme.accent;
        let config = ThemeConfig {
            name: "mocha".to_string(),
            accent: Some("#ff0000".to_string()),
            ..Default::default()
        };

        theme.apply_overrides(&config).expect("valid hex");

        assert_eq!(theme.accent, RgbColor::new(0xff, 0x00, 0x00));
        assert_eq!(theme.base, Theme::builtin("mocha").unwrap().base);
        assert_ne!(theme.accent, base);
    }

    #[test]
    fn invalid_hex_is_reported_and_the_slot_keeps_its_color() {
        let mut theme = Theme::builtin("mocha").expect("mocha ships with venti");
        let before = theme.accent;
        let config = ThemeConfig {
            accent: Some("red".to_string()),
            ..Default::default()
        };

        let err = theme.apply_overrides(&config).expect_err("bad hex");

        assert!(err.contains("accent"), "the error names the slot: {err}");
        assert_eq!(theme.accent, before);
    }

    #[test]
    fn resolve_applies_overrides_and_ignores_unknown_names() {
        let config = ThemeConfig {
            accent: Some("#010203".to_string()),
            ..Default::default()
        };
        let theme = Theme::resolve(&config);
        assert_eq!(theme.accent, RgbColor::new(1, 2, 3));

        let config = ThemeConfig {
            name: "no-such-theme".to_string(),
            ..config.clone()
        };
        let fallback = Theme::resolve(&config);
        assert_eq!(fallback.accent, RgbColor::new(1, 2, 3));
    }
}
