//! The listener's settings, stored as JSON next to the song database.

use std::fs;
use std::io;
use std::path::Path;

use media_session::SourcePreference;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Layout {
    Caption,
    Focus,
    #[default]
    Context,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Language {
    #[default]
    System,
    De,
    En,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Corner {
    TopLeft,
    TopRight,
    BottomLeft,
    BottomRight,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Settings {
    pub layout: Layout,
    pub font_size: f64,
    pub background_opacity: f64,
    pub overlay_opacity: f64,
    pub width: f64,
    pub height: f64,
    /// Seconds; positive shows the lyrics earlier.
    pub global_offset: f64,
    pub lookup_online: bool,
    pub source: SourcePreference,
    pub include_other_players: bool,
    pub click_through: bool,
    pub language: Language,
    pub overlay_visible: bool,
    /// Top-left corner in logical pixels, once the overlay was moved.
    pub overlay_position: Option<[f64; 2]>,
    pub hotkey: String,
}

pub const DEFAULT_HOTKEY: &str = "Alt+Shift+L";

impl Default for Settings {
    fn default() -> Self {
        Self {
            layout: Layout::Context,
            font_size: 25.0,
            background_opacity: 0.78,
            overlay_opacity: 1.0,
            width: 420.0,
            height: 240.0,
            global_offset: 0.0,
            lookup_online: true,
            source: SourcePreference::Automatic,
            include_other_players: false,
            click_through: false,
            language: Language::System,
            overlay_visible: true,
            overlay_position: None,
            hotkey: DEFAULT_HOTKEY.to_owned(),
        }
    }
}

impl Settings {
    /// Reads the settings, falling back to the defaults for anything missing or unreadable.
    pub fn load(path: &Path) -> Self {
        fs::read_to_string(path)
            .ok()
            .and_then(|text| serde_json::from_str::<Settings>(&text).ok())
            .unwrap_or_default()
            .sanitized()
    }

    /// Writes the settings atomically, so a crash never leaves half a file.
    pub fn save(&self, path: &Path) -> io::Result<()> {
        if let Some(directory) = path.parent() {
            fs::create_dir_all(directory)?;
        }
        let temporary = path.with_extension("json.tmp");
        let text = serde_json::to_string_pretty(self).map_err(io::Error::other)?;
        fs::write(&temporary, text)?;
        fs::rename(&temporary, path)
    }

    /// Applies the known fields of a JSON object. Unknown fields are ignored and
    /// a value of the wrong type rejects the whole change.
    pub fn merged(&self, patch: &serde_json::Value) -> Result<Self, String> {
        let serde_json::Value::Object(changes) = patch else {
            return Err("settings changes must be an object".into());
        };
        let mut value = serde_json::to_value(self).map_err(|error| error.to_string())?;
        if let serde_json::Value::Object(current) = &mut value {
            for (key, change) in changes {
                if current.contains_key(key) {
                    current.insert(key.clone(), change.clone());
                }
            }
        }
        serde_json::from_value::<Settings>(value)
            .map(Settings::sanitized)
            .map_err(|error| error.to_string())
    }

    /// Clamps every value into the range the interface offers.
    pub fn sanitized(mut self) -> Self {
        fn clamp(value: f64, min: f64, max: f64, fallback: f64) -> f64 {
            if value.is_finite() {
                value.clamp(min, max)
            } else {
                fallback
            }
        }
        let defaults = Settings::default();
        self.font_size = clamp(self.font_size, 14.0, 44.0, defaults.font_size).round();
        self.background_opacity = clamp(
            self.background_opacity,
            0.0,
            1.0,
            defaults.background_opacity,
        );
        self.overlay_opacity = clamp(self.overlay_opacity, 0.15, 1.0, defaults.overlay_opacity);
        self.width = clamp(self.width, 260.0, 800.0, defaults.width).round();
        self.height = clamp(self.height, 160.0, 500.0, defaults.height).round();
        self.global_offset = (clamp(self.global_offset, -5.0, 5.0, 0.0) * 10.0).round() / 10.0;
        self.overlay_position = self
            .overlay_position
            .filter(|[x, y]| x.is_finite() && y.is_finite());
        if self.hotkey.trim().is_empty() {
            self.hotkey = DEFAULT_HOTKEY.to_owned();
        }
        self
    }

    /// Width and height of the overlay in logical pixels for the chosen layout.
    pub fn overlay_size(&self) -> (f64, f64) {
        let height = match self.layout {
            Layout::Caption => self.height.min(150.0),
            Layout::Focus => self.height.min(205.0),
            Layout::Context => self.height,
        };
        (self.width, height)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn patches_only_known_fields_and_clamps_them() {
        let settings = Settings::default();
        let changed = settings
            .merged(&json!({ "fontSize": 90, "globalOffset": 1.234, "unknown": true, "layout": "caption" }))
            .expect("valid change");
        assert_eq!(changed.font_size, 44.0);
        assert_eq!(changed.global_offset, 1.2);
        assert_eq!(changed.layout, Layout::Caption);
        assert_eq!(changed.overlay_size(), (420.0, 150.0));
        assert!(
            settings.merged(&json!({ "fontSize": "large" })).is_err(),
            "wrong types are rejected"
        );
        assert!(settings.merged(&json!([1, 2])).is_err());
    }

    #[test]
    fn missing_or_broken_files_give_defaults() {
        let directory =
            std::env::temp_dir().join(format!("lyrics-overlay-settings-{}", std::process::id()));
        let path = directory.join("settings.json");
        assert_eq!(Settings::load(&path), Settings::default());

        let settings = Settings {
            source: SourcePreference::AppleMusic,
            overlay_position: Some([12.0, 34.0]),
            ..Settings::default()
        };
        settings.save(&path).expect("saved");
        assert_eq!(Settings::load(&path), settings);

        fs::write(&path, "{ not json").expect("written");
        assert_eq!(Settings::load(&path), Settings::default());
        fs::write(
            &path,
            r#"{"width": 9999, "overlayPosition": [1, 2], "hotkey": " "}"#,
        )
        .expect("written");
        let loaded = Settings::load(&path);
        assert_eq!(
            (
                loaded.width,
                loaded.overlay_position,
                loaded.hotkey.as_str()
            ),
            (800.0, Some([1.0, 2.0]), DEFAULT_HOTKEY)
        );
        let _ = fs::remove_dir_all(directory);
    }
}
