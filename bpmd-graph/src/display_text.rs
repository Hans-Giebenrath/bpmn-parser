use cosmic_text::{Buffer, FontSystem, Metrics, SwashCache};

use crate::MAX_NODE_WIDTH;

/// Meant for node and edge texts (although it is kinda redundant for activity nodes).
/// They can be positioned at different locations, wherever there is room.
/// For pool and lane titles this is not used, as there is only one way to put them, so
/// their placement is trivial (and due to the rotation by itself special).
#[derive(Debug, Clone)]
pub struct DisplayText {
    pub raw_text: String,
    /// Only set in the display text layout phase.
    pub location: DisplayTextLocation,
    pub max_width: Option<u16>,
    pub font_size: f32,
    pub line_height: f32,
    pub font_family: String,
    /// An empty string means "black".
    /// TODO Should probably be some interned string, no need to repeat the same colors over and over.
    /// Or should it just be the
    pub font_color: String,
    /// Only set in the display text layout phase. Contains the finished rendering.
    pub buffer: Buffer,
}

impl DisplayText {
    pub fn new(raw_text: String) -> Self {
        Self {
            raw_text,
            location: DisplayTextLocation {
                alignment: Alignment::Left,
                x: 0,
                y: 0,
            },
            max_width: Some(MAX_NODE_WIDTH as u16),
            font_size: 12.0,
            line_height: 14.0,
            font_family: "sans-serif".to_string(),
            font_color: "#111".to_string(),
            buffer: Buffer::new_empty(Metrics::new(12.0, 14.0)),
        }
    }
}

#[derive(Debug, Clone)]
pub struct DisplayTextLocation {
    pub alignment: Alignment,
    pub x: usize,
    pub y: usize,
}

#[derive(Debug, Clone, Copy)]
pub enum Alignment {
    Left,
    Center,
    Right,
}

pub struct FontCache {
    pub font_system: FontSystem,
    pub swash_cache: SwashCache,
}

impl FontCache {
    pub fn new() -> Self {
        let mut font_system = FontSystem::new();
        let swash_cache = SwashCache::new();
        font_system
            .db_mut()
            .load_font_data(include_bytes!("../../inter-font/Inter-Regular.ttf").to_vec());
        font_system
            .db_mut()
            .load_font_data(include_bytes!("../../inter-font/Inter-SemiBold.ttf").to_vec());
        font_system
            .db_mut()
            .load_font_data(include_bytes!("../../inter-font/Inter-Italic.ttf").to_vec());
        Self {
            font_system,
            swash_cache,
        }
    }
}

impl Default for FontCache {
    fn default() -> Self {
        Self::new()
    }
}
