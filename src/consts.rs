use raylib::color::Color;

pub const NODE_LINE_LENGTH: usize = 18;
pub const NODE_LINES: usize = 15;
pub const NODE_TEXT_BUFFER_SIZE: usize = (NODE_LINE_LENGTH + 1) * NODE_LINES;
pub const NODE_FONT_SIZE: f32 = 20.;
pub const NODE_LINE_HEIGHT: f32 = 20.;
pub const NODE_CHAR_WIDTH: f32 = (9. / 20.) * NODE_FONT_SIZE + NODE_FONT_SPACING;
pub const NODE_FONT_SPACING: f32 = 3.;
pub const NODE_INSIDE_SIDE_LENGTH: f32 = NODE_LINES as f32 * NODE_FONT_SIZE;
pub const NODE_INSIDE_PADDING: f32 = 10.;
pub const NODE_OUTSIDE_PADDING: f32 = 100.;
pub const NODE_OUTSIDE_SIDE_LENGTH: f32 = NODE_INSIDE_SIDE_LENGTH + 2. * NODE_INSIDE_PADDING;
pub const GHOST_NODE_DASHES: usize = 8;
pub const LINE_THICKNESS: f32 = 2.0;
pub const GIZMO_HEIGHT: f32 = NODE_OUTSIDE_SIDE_LENGTH / 4.0;
pub const GIZMO_WIDTH: f32 = NODE_OUTSIDE_SIDE_LENGTH - NODE_TEXT_BOX_OUTSIDE_WIDTH;
pub const NODE_TEXT_BOX_INSIDE_WIDTH: f32 = (NODE_LINE_LENGTH as f32 + 0.5) * NODE_CHAR_WIDTH;
pub const NODE_TEXT_BOX_OUTSIDE_WIDTH: f32 = NODE_TEXT_BOX_INSIDE_WIDTH + 2.0 * NODE_INSIDE_PADDING;
pub const KEY_REPEAT_DELAY_S: f32 = 0.5;
pub const KEY_REPEAT_INTERVAL_S: f32 = 1.0 / 30.0;
pub const GHOST_COLOR: Color = Color::GRAY;
