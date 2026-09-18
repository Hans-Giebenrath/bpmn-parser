use cosmic_text::Buffer;

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
