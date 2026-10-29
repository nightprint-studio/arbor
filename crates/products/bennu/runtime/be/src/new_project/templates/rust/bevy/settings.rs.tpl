use bevy::prelude::*;
use serde::Deserialize;

/// Read from `assets/data/settings.ron` at startup, before the app exists, because the window
/// title is needed to build it. Edit the file and run again.
#[derive(Resource, Deserialize, Debug, Clone)]
pub struct Settings {
    pub title: String,
    pub background: (f32, f32, f32),
}

impl Default for Settings {
    fn default() -> Self {
        Self { title: "{{title}}".into(), background: (0.1, 0.1, 0.12) }
    }
}

impl Settings {
    pub fn load() -> Self {
        std::fs::read_to_string("assets/data/settings.ron")
            .ok()
            .and_then(|text| ron::from_str(&text).ok())
            .unwrap_or_default()
    }

    pub fn background(&self) -> Color {
        Color::srgb(self.background.0, self.background.1, self.background.2)
    }
}
