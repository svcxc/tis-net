#![feature(array_repeat)]
#![feature(let_chains)]
#![feature(map_try_insert)]
#![feature(iterator_try_collect)]
#![feature(map_many_mut)]
#![feature(iter_intersperse)]

use std::{borrow::Cow, collections::HashMap, f32};

use arrayvec::{ArrayString, ArrayVec};
use raylib::prelude::*;

const NODE_LINE_LENGTH: usize = 18;
const NODE_LINES: usize = 15;
const NODE_TEXT_BUFFER_SIZE: usize = (NODE_LINE_LENGTH + 1) * NODE_LINES;
const NODE_FONT_SIZE: f32 = 20.;
const NODE_LINE_HEIGHT: f32 = 20.;
const NODE_CHAR_WIDTH: f32 = (9. / 20.) * NODE_FONT_SIZE + NODE_FONT_SPACING;
const NODE_FONT_SPACING: f32 = 3.;
const NODE_INSIDE_SIDE_LENGTH: f32 = NODE_LINES as f32 * NODE_FONT_SIZE;
const NODE_INSIDE_PADDING: f32 = 10.;
const NODE_OUTSIDE_PADDING: f32 = 100.;
const NODE_OUTSIDE_SIDE_LENGTH: f32 = NODE_INSIDE_SIDE_LENGTH + 2. * NODE_INSIDE_PADDING;
const GHOST_NODE_DASHES: usize = 8;
const LINE_THICKNESS: f32 = 2.0;
const GIZMO_OUTSIDE_SIDE_LENGTH: f32 = NODE_OUTSIDE_SIDE_LENGTH / 4.0;
const NODE_TEXT_BOX_WIDTH: f32 =
    NODE_OUTSIDE_SIDE_LENGTH - GIZMO_OUTSIDE_SIDE_LENGTH - NODE_INSIDE_PADDING * 2.0;

type Nodes = HashMap<NodeCoord, Node>;

struct Model {
    camera: Camera2D,
    nodes: Nodes,
    ghost_nodes: GhostNodes,
    highlighted_node: NodeCoord,
}

type GhostLocs = [Option<NodeCoord>; 4];

enum GhostNodes {
    CreateGhosts(GhostLocs),
    MoveGhosts(GhostLocs),
    None,
}

fn ghost_loc_coords(ghost_locs: &GhostLocs) -> impl Iterator<Item = NodeCoord> {
    ghost_locs.iter().flatten().copied()
}

fn ghost_loc_coords_directions(ghost_locs: &GhostLocs) -> impl Iterator<Item = (NodeCoord, Dir)> {
    ghost_locs
        .iter()
        .zip([Dir::Up, Dir::Down, Dir::Left, Dir::Right])
        .filter_map(|(coord, dir)| coord.map(|coord| (coord, dir)))
}

type NodeText = ArrayString<NODE_TEXT_BUFFER_SIZE>;

#[derive(Clone)]
struct Node {
    text: NodeText,
    cursor: usize,
    error: Option<ParseErr>,
    exec: Option<NodeExec>,
}

impl Node {
    fn empty() -> Self {
        Self {
            text: ArrayString::new(),
            cursor: 0,
            error: None,
            exec: None,
        }
    }

    fn is_empty(&self) -> bool {
        self.text.is_empty()
    }

    fn with_text(str: &str) -> Self {
        let text = ArrayString::from(str).unwrap();

        assert!(validate(&text));

        let mut new = Self {
            text,
            cursor: 0,
            error: None,
            exec: None,
        };

        new.update_error();

        new
    }

    fn is_in_edit_mode(&self) -> bool {
        self.exec.is_none()
    }

    fn backspace(&mut self) {
        let Some(index) = self.cursor.checked_sub(1) else {
            return;
        };

        self.text.remove(index);
        self.cursor = index;
    }

    fn insert(&mut self, char: char) {
        let mut new_text = ArrayString::new();

        new_text.push_str(&self.text[..self.cursor]);
        new_text.push(char);
        new_text.push_str(&self.text[self.cursor..]);

        if validate(&new_text) {
            self.text = new_text;
            self.cursor += 1;
        }
    }

    fn enter(&mut self) {
        self.insert('\n');
    }

    fn right(&mut self) {
        self.cursor = usize::min(self.cursor + 1, self.text.len());
    }

    fn left(&mut self) {
        self.cursor = self.cursor.saturating_sub(1);
    }

    fn target(&mut self, target_line: usize, target_column: usize) {
        let mut chars = self.text.chars();
        let mut line = 0;
        let mut column = 0;
        let mut cursor = 0;

        while line < target_line
            && let Some(char) = chars.next()
        {
            if char == '\n' {
                line += 1;
            }
            cursor += 1;
        }

        while column < target_column
            && let Some(char) = chars.next()
        {
            if char == '\n' {
                break;
            } else {
                cursor += 1;
                column += 1;
            }
        }

        self.cursor = cursor;
    }

    fn up(&mut self) {
        let (line, target_column) = line_column(&self.text, self.cursor);

        let Some(target_line) = line.checked_sub(1) else {
            return;
        };

        self.target(target_line, target_column)
    }

    fn down(&mut self) {
        let (line, target_column) = line_column(&self.text, self.cursor);

        let target_line = line + 1;

        self.target(target_line, target_column)
    }

    fn home(&mut self) {
        let mut cursor = self.cursor;

        for char in self.text.chars().rev().skip(self.text.len() - self.cursor) {
            if char == '\n' {
                break;
            } else {
                cursor -= 1;
            }
        }

        self.cursor = cursor;
    }

    fn end(&mut self) {
        let mut cursor = self.cursor;

        for char in self.text.chars().skip(self.cursor) {
            if char == '\n' {
                break;
            } else {
                cursor += 1;
            }
        }

        self.cursor = cursor;
    }

    fn update_edit(&mut self, pressed: &Pressed) {
        match pressed {
            Pressed::Arrow(Dir::Up) => self.up(),
            Pressed::Arrow(Dir::Down) => self.down(),
            Pressed::Arrow(Dir::Left) => self.left(),
            Pressed::Arrow(Dir::Right) => self.right(),
            Pressed::Tab => {} // TODO: TAB and ESC are the only buttons here that can't be used in editing; move them out of this enum in the future
            Pressed::Esc => {}
            Pressed::Enter => self.enter(),
            Pressed::Backspace => self.backspace(),
            Pressed::Home => self.home(),
            Pressed::End => self.end(),
            Pressed::Char(char) => self.insert(*char),
        }

        self.update_error();
    }

    fn update_error(&mut self) {
        self.error = if let Err(parse_err) = parse_node_text(&self.text) {
            Some(parse_err)
        } else {
            None
        }
    }
}

fn validate(node_text: &NodeText) -> bool {
    node_text
        .split('\n')
        .all(|line| line.len() <= NODE_LINE_LENGTH)
        && node_text.split('\n').count() <= NODE_LINES
}

fn line_column(str: &str, index: usize) -> (usize, usize) {
    assert!(index <= str.len());

    let mut line = 0;
    let mut column = 0;

    for char in str.chars().take(index) {
        if char == '\n' {
            line += 1;
            column = 0;
        } else {
            column += 1;
        }
    }

    (line, column)
}

#[derive(PartialEq, Eq, Hash, Clone, Copy, Debug)]
struct NodeCoord {
    x: isize,
    y: isize,
}

impl NodeCoord {
    fn at(x: isize, y: isize) -> Self {
        Self { x, y }
    }

    fn top_left_corner(&self) -> Vector2 {
        Vector2 {
            x: self.x as f32,
            y: self.y as f32,
        }
        .scale_by(NODE_OUTSIDE_SIDE_LENGTH + NODE_OUTSIDE_PADDING)
    }

    fn top_right_corner(&self) -> Vector2 {
        self.top_left_corner()
            + Vector2 {
                x: NODE_OUTSIDE_SIDE_LENGTH,
                y: 0.,
            }
    }

    fn bottom_left_corner(&self) -> Vector2 {
        self.top_left_corner()
            + Vector2 {
                x: 0.,
                y: NODE_OUTSIDE_SIDE_LENGTH,
            }
    }

    fn bottom_right_corner(&self) -> Vector2 {
        self.top_left_corner()
            + Vector2 {
                x: NODE_OUTSIDE_SIDE_LENGTH,
                y: NODE_OUTSIDE_SIDE_LENGTH,
            }
    }

    fn text_loc(&self) -> Vector2 {
        self.top_left_corner() + Vector2::one().scale_by(NODE_INSIDE_PADDING)
    }

    fn line_pos(&self, line_number: usize) -> Vector2 {
        self.top_left_corner()
            + Vector2::one().scale_by(NODE_INSIDE_PADDING)
            + Vector2::new(0., line_number as f32 * NODE_LINE_HEIGHT)
    }

    fn center(&self) -> Vector2 {
        self.top_left_corner() + Vector2::one().scale_by(NODE_OUTSIDE_SIDE_LENGTH / 2.)
    }

    fn neighbor(self, direction: Dir) -> Self {
        let NodeCoord { x, y } = self;

        match direction {
            Dir::Up => NodeCoord { x, y: y - 1 },
            Dir::Down => NodeCoord { x, y: y + 1 },
            Dir::Left => NodeCoord { x: x - 1, y },
            Dir::Right => NodeCoord { x: x + 1, y },
        }
    }
}

fn main() {
    let (mut rl, thread) = raylib::init().resizable().title("Hello, World").build();

    rl.set_target_fps(60);
    rl.set_text_line_spacing(NODE_LINE_HEIGHT as _);
    rl.set_exit_key(None);

    let font = rl
        .load_font_from_memory(
            &thread,
            ".ttf",
            // include_bytes!("RobotoMono-Light.ttf"),
            include_bytes!("RobotoMono-Medium.ttf"),
            40,
            None,
        )
        .unwrap();

    font.texture()
        .set_texture_filter(&thread, TextureFilter::TEXTURE_FILTER_BILINEAR);

    let mut model = init();

    loop {
        if rl.window_should_close() {
            break;
        }

        let input = get_input(&mut rl);

        let Some(new_model) = update(model, input) else {
            break;
        };

        render(&mut rl, &thread, &new_model, &font);

        model = new_model;
    }
}

fn init() -> Model {
    let highlighted_node = NodeCoord::at(0, 0);

    let camera = Camera2D {
        offset: Default::default(),
        target: Default::default(),
        rotation: Default::default(),
        zoom: 0.85,
    };

    let sender = Node::with_text(
        ["MOV 69 RIGHT", "A:JMP A"]
            .into_iter()
            .intersperse("\n")
            .collect::<String>()
            .as_str(),
    );

    let receiver = Node::with_text(
        ["MOV LEFT ACC", "A:JMP A"]
            .into_iter()
            .intersperse("\n")
            .collect::<String>()
            .as_str(),
    );

    let nodes = HashMap::from([
        (highlighted_node, sender),
        (highlighted_node.neighbor(Dir::Right), receiver),
    ]);

    Model {
        camera,
        nodes,
        ghost_nodes: GhostNodes::None,
        highlighted_node,
    }
}

fn render(rl: &mut RaylibHandle, thread: &RaylibThread, model: &Model, font: &Font) {
    let mut d = rl.begin_drawing(&thread);
    let mut d = d.begin_mode2D(model.camera);
    let d = &mut d;

    d.clear_background(Color::BLACK);

    render_nodes(d, model, font);

    render_ghost_nodes(d, &model.ghost_nodes);

    let highlighted = model
        .nodes
        .get(&model.highlighted_node)
        .expect("highlighted node should always exist");

    if highlighted.is_in_edit_mode() {
        render_cursor(d, model.highlighted_node, highlighted);
    }
}

fn render_nodes(d: &mut impl RaylibDraw, model: &Model, font: &Font) {
    for (node_loc, node) in model.nodes.iter() {
        let line_color = if node_loc == &model.highlighted_node {
            Color::WHITE
        } else {
            Color::GRAY
        };

        render_node_border(d, *node_loc, line_color);

        render_node_gizmos(d, *node_loc, &node.exec, font, line_color, Color::GRAY);

        d.draw_text_ex(
            font,
            &node.text,
            node_loc.text_loc(),
            NODE_FONT_SIZE,
            NODE_FONT_SPACING,
            Color::WHITE,
        );

        // the below two things should not be true at the same time if I did my homework
        // (because a node with an error should not be able to begin executing)
        // but this isn't reflected in the type system. If it were to happen though, it means there's a bug
        debug_assert!(!(node.error.is_some() && node.exec.is_some()));

        if let Some(error) = &node.error
            && show_error(node_loc, node, &model.highlighted_node, error.line)
        {
            render_error_squiggle(d, *node_loc, &node.text, error.line);
        }

        if let Some(exec) = &node.exec
            && !exec.code.is_empty()
        {
            let highlight_color = if exec.io == NodeIO::None {
                Color::WHITE
            } else {
                Color::GRAY
            };

            render_highlighted_line(d, node_loc, node, font, &highlight_color);

            if let NodeIO::Outbound(dir, value) = exec.io {
                render_io_arrow(d, node_loc, dir, &value.to_string(), font);
            } else if let NodeIO::Inbound(io_dir) = exec.io
                && !neighbor_sending_io(&model.nodes, node_loc, io_dir)
            {
                render_io_arrow(d, &node_loc.neighbor(io_dir), io_dir.inverse(), "?", font);
            }
        }
    }

    for (node_loc, node) in model.nodes.iter() {
        if let Some(error) = &node.error
            && show_error(node_loc, node, &model.highlighted_node, error.line)
        {
            render_error_msg(d, node_loc, &error.problem, font);
        };
    }
}

fn show_error(
    node_loc: &NodeCoord,
    node: &Node,
    highlighted_node: &NodeCoord,
    error_line: u8,
) -> bool {
    node_loc != highlighted_node || line_column(&node.text, node.cursor).0 != error_line as usize
}

fn render_error_msg(
    d: &mut impl RaylibDraw,
    node_loc: &NodeCoord,
    problem: &ParseProblem,
    font: &Font,
) {
    const BOX_HEIGHT: f32 = NODE_LINE_HEIGHT + 2.0 * NODE_INSIDE_PADDING;

    const BOX_NODE_PADDING: f32 = 0.25 * (NODE_OUTSIDE_PADDING - BOX_HEIGHT);

    let bottom_left = node_loc.top_left_corner() - Vector2::new(0.0, BOX_NODE_PADDING);

    let top_left = bottom_left - Vector2::new(0.0, BOX_HEIGHT);

    let top_right = top_left + Vector2::new(NODE_OUTSIDE_SIDE_LENGTH, 0.0);
    let bottom_right = bottom_left + Vector2::new(NODE_OUTSIDE_SIDE_LENGTH, 0.0);

    let center = top_left + Vector2::new(0.5 * NODE_OUTSIDE_SIDE_LENGTH, 0.5 * BOX_HEIGHT);

    d.draw_rectangle_v(top_left, bottom_right - top_left, Color::BLACK);

    d.draw_line_ex(top_left, top_right, LINE_THICKNESS, Color::RED);
    d.draw_line_ex(top_left, bottom_left, LINE_THICKNESS, Color::RED);
    d.draw_line_ex(bottom_left, bottom_right, LINE_THICKNESS, Color::RED);
    d.draw_line_ex(top_right, bottom_right, LINE_THICKNESS, Color::RED);

    render_centered_text(d, problem.to_str(), center, font, Color::RED);
}

fn neighbor_sending_io(nodes: &Nodes, node_loc: &NodeCoord, io_dir: Dir) -> bool {
    let Some(neighbor) = nodes.get(&node_loc.neighbor(io_dir)) else {
        return false;
    };

    let Some(neighbor_exec) = &neighbor.exec else {
        return false;
    };

    let NodeIO::Outbound(neighbor_io_dir, _) = neighbor_exec.io else {
        return false;
    };

    neighbor_io_dir == io_dir.inverse()
}

fn render_node_gizmos(
    d: &mut impl RaylibDraw,
    node_loc: NodeCoord,
    exec: &Option<NodeExec>,
    font: &Font,
    primary: Color,
    secondary: Color,
) {
    let (acc_string, bak_string);

    let (acc, bak, mode) = if let Some(exec) = exec {
        acc_string = exec.acc.to_string();

        bak_string = if exec.bak < -99 {
            exec.bak.to_string()
        } else {
            format!("({})", exec.bak)
        };

        let mode_str = match exec.io {
            NodeIO::None => "EXEC",
            NodeIO::Inbound(_) => "READ",
            NodeIO::Outbound(_, _) => "WRTE",
        };

        (acc_string.as_str(), bak_string.as_str(), mode_str)
    } else {
        ("0", "(0)", "EDIT")
    };

    let placeholder_gizmos = [("ACC", acc), ("BAK", bak), ("LAST", "N/A"), ("MODE", mode)];

    for (i, (top, bottom)) in placeholder_gizmos.into_iter().enumerate() {
        let gizmos_top_left = node_loc.top_right_corner()
            - Vector2::new(
                GIZMO_OUTSIDE_SIDE_LENGTH,
                i as f32 * -GIZMO_OUTSIDE_SIDE_LENGTH,
            );

        let left_right = Vector2::new(GIZMO_OUTSIDE_SIDE_LENGTH, 0.0);
        let top_down = Vector2::new(0.0, GIZMO_OUTSIDE_SIDE_LENGTH);

        // draws a rectangle out of individual lines
        // doing this makes the lines centered, rather than aligned to the outside
        d.draw_line_ex(
            gizmos_top_left,
            gizmos_top_left + left_right,
            LINE_THICKNESS,
            primary,
        );
        d.draw_line_ex(
            gizmos_top_left,
            gizmos_top_left + top_down,
            LINE_THICKNESS,
            primary,
        );
        d.draw_line_ex(
            gizmos_top_left + left_right,
            gizmos_top_left + left_right + top_down,
            LINE_THICKNESS,
            primary,
        );
        d.draw_line_ex(
            gizmos_top_left + top_down,
            gizmos_top_left + top_down + left_right,
            LINE_THICKNESS,
            primary,
        );

        let text_center = gizmos_top_left
            + Vector2::new(
                GIZMO_OUTSIDE_SIDE_LENGTH / 2.,
                GIZMO_OUTSIDE_SIDE_LENGTH / 2.,
            );
        let text_offset = Vector2::new(0.0, NODE_LINE_HEIGHT / 2.0);
        let top_text = text_center - text_offset;
        let bottom_text = text_center + text_offset;

        render_centered_text(d, top, top_text, font, secondary);
        render_centered_text(d, bottom, bottom_text, font, Color::WHITE);
    }
}

fn render_cursor(d: &mut impl RaylibDraw, node_loc: NodeCoord, node: &Node) {
    let (line, column) = line_column(&node.text, node.cursor);

    let x_offset = column as f32 * NODE_CHAR_WIDTH;

    let cursor_top = node_loc.line_pos(line) + Vector2::new(x_offset, 0.);
    let cursor_bottom = cursor_top + Vector2::new(0., NODE_LINE_HEIGHT);

    d.draw_line_ex(cursor_top, cursor_bottom, LINE_THICKNESS, Color::WHITE);
}

fn render_error_squiggle(
    d: &mut impl RaylibDraw,
    node_loc: NodeCoord,
    node_text: &NodeText,
    line_no: u8,
) {
    let Some(line_len) = node_text.lines().nth(line_no as usize).map(str::len) else {
        return;
    };

    let squiggle_start = node_loc.line_pos(line_no as usize) + Vector2::new(0.0, NODE_LINE_HEIGHT);
    let squiggle_end = squiggle_start + Vector2::new(line_len as f32 * NODE_CHAR_WIDTH, 0.0);

    d.draw_line_ex(squiggle_start, squiggle_end, LINE_THICKNESS, Color::RED);
}

fn render_io_arrow(
    d: &mut impl RaylibDraw,
    node_loc: &NodeCoord,
    dir: Dir,
    label: &str,
    font: &Font,
) {
    let indicator_center = node_loc.center() + dir.io_indicator();

    let component_offset = dir.normalized().scale_by(1. / 6. * NODE_OUTSIDE_PADDING);

    let arrow_center = indicator_center + component_offset;
    let text_center = indicator_center - component_offset;

    render_arrow(d, arrow_center, dir, Color::WHITE);

    render_centered_text(d, label, text_center, font, Color::WHITE);
}

fn render_highlighted_line(
    d: &mut impl RaylibDraw,
    node_loc: &NodeCoord,
    node: &Node,
    font: &Font,
    color: &Color,
) {
    if let Some(NodeExec { ip, ref code, .. }) = node.exec {
        let line_no = code[ip as usize].src_line as usize;

        let line_pos = node_loc.line_pos(line_no);

        let highlight_pos = line_pos
            - Vector2 {
                x: NODE_INSIDE_PADDING * 0.25,
                y: 0.0,
            };

        const HIGHLIGHT_SIZE: Vector2 = Vector2 {
            x: NODE_TEXT_BOX_WIDTH + NODE_INSIDE_PADDING * 0.5,
            y: NODE_LINE_HEIGHT,
        };

        d.draw_rectangle_v(highlight_pos, HIGHLIGHT_SIZE, color);

        d.draw_text_ex(
            font,
            &node.text.lines().nth(line_no).unwrap_or(""),
            line_pos,
            NODE_FONT_SIZE,
            NODE_FONT_SPACING,
            Color::BLACK,
        );
    }
}

fn render_ghost_nodes(d: &mut impl RaylibDraw, ghost_nodes: &GhostNodes) {
    const GHOST_COLOR: Color = Color::GRAY;

    match ghost_nodes {
        GhostNodes::None => {}

        GhostNodes::CreateGhosts(ghost_locs) => {
            for node_loc in ghost_loc_coords(ghost_locs) {
                render_dashed_node_border(d, node_loc, GHOST_COLOR);
            }
        }

        GhostNodes::MoveGhosts(ghost_locs) => {
            for (node_loc, direction) in ghost_loc_coords_directions(ghost_locs) {
                render_dashed_node_border(d, node_loc, GHOST_COLOR);

                render_arrow(d, node_loc.center(), direction, GHOST_COLOR);
            }
        }
    }
}

fn render_dashed_line(
    d: &mut impl RaylibDraw,
    start_pos: Vector2,
    end_pos: Vector2,
    color: Color,
    dashes: usize,
) {
    let dash_len = NODE_OUTSIDE_SIDE_LENGTH / (2 * GHOST_NODE_DASHES + 1) as f32;

    let dash_tail = (end_pos - start_pos).normalized().scale_by(dash_len);

    for dash_no in 0..=dashes {
        let dash_start = start_pos + dash_tail.scale_by(2.0 * dash_no as f32);
        d.draw_line_ex(dash_start, dash_start + dash_tail, LINE_THICKNESS, color);
    }
}

#[derive(PartialEq, Eq, Clone, Copy, Debug)]
enum Dir {
    Up,
    Down,
    Left,
    Right,
}

impl Dir {
    const ALL: [Self; 4] = [Dir::Up, Dir::Down, Dir::Left, Dir::Right];

    fn normalized(&self) -> Vector2 {
        match self {
            Dir::Up => Vector2::new(0.0, -1.0),
            Dir::Down => Vector2::new(0.0, 1.0),
            Dir::Left => Vector2::new(-1.0, 0.0),
            Dir::Right => Vector2::new(1.0, 0.0),
        }
    }

    fn io_indicator(&self) -> Vector2 {
        const OFF_CENTER: f32 = NODE_OUTSIDE_SIDE_LENGTH / 6.0;
        const BETWEEN_NODES: f32 = 0.5 * (NODE_OUTSIDE_SIDE_LENGTH + NODE_OUTSIDE_PADDING);

        match self {
            Dir::Up => Vector2 {
                x: OFF_CENTER,
                y: -BETWEEN_NODES,
            },
            Dir::Down => Vector2 {
                x: -OFF_CENTER,
                y: BETWEEN_NODES,
            },
            Dir::Left => Vector2 {
                x: -BETWEEN_NODES,
                y: -OFF_CENTER,
            },
            Dir::Right => Vector2 {
                x: BETWEEN_NODES,
                y: OFF_CENTER,
            },
        }
    }

    fn inverse(&self) -> Self {
        match self {
            Dir::Up => Dir::Down,
            Dir::Down => Dir::Up,
            Dir::Left => Dir::Right,
            Dir::Right => Dir::Left,
        }
    }
}

fn render_arrow(d: &mut impl RaylibDraw, center: Vector2, direction: Dir, color: Color) {
    let dir_vec = direction.normalized();

    let arrow_tip = center + dir_vec.scale_by(NODE_LINE_HEIGHT);
    let arrow_base = center - dir_vec.scale_by(NODE_LINE_HEIGHT);

    let arrow_left_wing = center
        + dir_vec
            .scale_by(NODE_LINE_HEIGHT)
            .rotated((1.0 / 4.0) * f32::consts::TAU);

    let arrow_right_wing = center
        + dir_vec
            .scale_by(NODE_LINE_HEIGHT)
            .rotated(-(1.0 / 4.0) * f32::consts::TAU);

    d.draw_line_ex(arrow_base, arrow_tip, LINE_THICKNESS, color);
    d.draw_line_ex(arrow_tip, arrow_left_wing, LINE_THICKNESS, color);
    d.draw_line_ex(arrow_tip, arrow_right_wing, LINE_THICKNESS, color);
}

fn render_node_border(d: &mut impl RaylibDraw, node_loc: NodeCoord, line_color: Color) {
    d.draw_line_ex(
        node_loc.top_left_corner(),
        node_loc.top_right_corner(),
        LINE_THICKNESS,
        line_color,
    );
    d.draw_line_ex(
        node_loc.top_left_corner(),
        node_loc.bottom_left_corner(),
        LINE_THICKNESS,
        line_color,
    );
    d.draw_line_ex(
        node_loc.bottom_left_corner(),
        node_loc.bottom_right_corner(),
        LINE_THICKNESS,
        line_color,
    );
    d.draw_line_ex(
        node_loc.top_right_corner(),
        node_loc.bottom_right_corner(),
        LINE_THICKNESS,
        line_color,
    );
}

fn render_centered_text(
    d: &mut impl RaylibDraw,
    text: &str,
    center: Vector2,
    font: &Font,
    color: Color,
) {
    let text_size = font.measure_text(text, NODE_FONT_SIZE, NODE_FONT_SPACING);

    let top_left = center - text_size.scale_by(0.5);

    d.draw_text_ex(
        font,
        text,
        top_left,
        NODE_FONT_SIZE,
        NODE_FONT_SPACING,
        color,
    );
}

fn render_dashed_node_border(d: &mut impl RaylibDraw, node_loc: NodeCoord, line_color: Color) {
    render_dashed_line(
        d,
        node_loc.top_left_corner(),
        node_loc.top_right_corner(),
        line_color,
        GHOST_NODE_DASHES,
    );
    render_dashed_line(
        d,
        node_loc.top_left_corner(),
        node_loc.bottom_left_corner(),
        line_color,
        GHOST_NODE_DASHES,
    );
    render_dashed_line(
        d,
        node_loc.bottom_left_corner(),
        node_loc.bottom_right_corner(),
        line_color,
        GHOST_NODE_DASHES,
    );
    render_dashed_line(
        d,
        node_loc.top_right_corner(),
        node_loc.bottom_right_corner(),
        line_color,
        GHOST_NODE_DASHES,
    );
}

#[derive(Clone, Copy)]
struct Input {
    ctrl_held: bool,
    shift_held: bool,
    pressed: Option<Pressed>,
    window_dimensions: (i32, i32),
    mouse_wheel_move: f32,
}

#[derive(PartialEq, Eq, Clone, Copy)]
enum Pressed {
    Esc,
    Tab,
    Backspace,
    Enter,
    Home,
    End,
    Arrow(Dir),
    Char(char),
}

fn get_input(rl: &mut RaylibHandle) -> Input {
    let ctrl_held = rl.is_key_down(KeyboardKey::KEY_LEFT_CONTROL);
    let shift_held = rl.is_key_down(KeyboardKey::KEY_LEFT_SHIFT);

    const GHETTO_HOME: KeyboardKey = KeyboardKey::KEY_KP_7;
    const GHETTO_END: KeyboardKey = KeyboardKey::KEY_KP_1;

    let pressed = if rl.is_key_pressed(KeyboardKey::KEY_TAB) {
        Some(Pressed::Tab)
    } else if rl.is_key_pressed(KeyboardKey::KEY_ESCAPE) {
        Some(Pressed::Esc)
    } else if rl.is_key_pressed(KeyboardKey::KEY_BACKSPACE) {
        Some(Pressed::Backspace)
    } else if rl.is_key_pressed(KeyboardKey::KEY_ENTER) {
        Some(Pressed::Enter)
    } else if rl.is_key_pressed(KeyboardKey::KEY_HOME) || rl.is_key_pressed(GHETTO_HOME) {
        Some(Pressed::Home)
    } else if rl.is_key_pressed(KeyboardKey::KEY_END) || rl.is_key_pressed(GHETTO_END) {
        Some(Pressed::End)
    } else if rl.is_key_pressed(KeyboardKey::KEY_UP) {
        Some(Pressed::Arrow(Dir::Up))
    } else if rl.is_key_pressed(KeyboardKey::KEY_DOWN) {
        Some(Pressed::Arrow(Dir::Down))
    } else if rl.is_key_pressed(KeyboardKey::KEY_LEFT) {
        Some(Pressed::Arrow(Dir::Left))
    } else if rl.is_key_pressed(KeyboardKey::KEY_RIGHT) {
        Some(Pressed::Arrow(Dir::Right))
    } else if let Some(char) = rl.get_char_pressed() {
        Some(Pressed::Char(char.to_ascii_uppercase()))
    } else {
        None
    };

    Input {
        ctrl_held,
        shift_held,
        pressed,
        window_dimensions: (rl.get_screen_width(), rl.get_screen_height()),
        mouse_wheel_move: rl.get_mouse_wheel_move(),
    }
}

fn update(model: Model, input: Input) -> Option<Model> {
    let (nodes, ghost_nodes, highlighted_node) = match handle_input(&model, &input) {
        HandledInput::Exit => return None,
        HandledInput::NoChange => (model.nodes, model.ghost_nodes, model.highlighted_node),
        HandledInput::NodesChanged(nodes) => (nodes, model.ghost_nodes, model.highlighted_node),
        HandledInput::ViewChanged(ghost_nodes, highlighted_node) => {
            (model.nodes, ghost_nodes, highlighted_node)
        }
        HandledInput::EverythingChanged(nodes, ghost_nodes, highlighted_node) => {
            (nodes, ghost_nodes, highlighted_node)
        }
    };

    let camera = update_camera(
        model.camera,
        highlighted_node,
        input.window_dimensions,
        input.mouse_wheel_move,
    );

    Some(Model {
        camera,
        nodes,
        ghost_nodes,
        highlighted_node,
    })
}

enum HandledInput {
    Exit,
    NoChange,
    ViewChanged(GhostNodes, NodeCoord),
    NodesChanged(Nodes),
    EverythingChanged(Nodes, GhostNodes, NodeCoord),
}

fn handle_input(model: &Model, input: &Input) -> HandledInput {
    match input {
        Input {
            pressed: Some(Pressed::Esc),
            ..
        } => {
            if let Some(updated_nodes) = stop_execution(&model.nodes, model.highlighted_node) {
                let mut nodes = model.nodes.clone();
                nodes.extend(updated_nodes);
                HandledInput::NodesChanged(nodes)
            } else {
                HandledInput::Exit
            }
        }

        Input {
            ctrl_held: false,
            shift_held: false,
            pressed: Some(Pressed::Tab),
            ..
        } => {
            if let Some(updated_nodes) = step_execution(&model.nodes, model.highlighted_node) {
                let mut nodes = model.nodes.clone();
                nodes.extend(updated_nodes);
                HandledInput::NodesChanged(nodes)
            } else {
                HandledInput::NoChange
            }
        }

        Input {
            ctrl_held: false,
            pressed: Some(pressed),
            ..
        } => {
            // potential optimization: a special case of HandledInput could be made
            // in case only the currently highlighted node has changed, as it has here.
            // currently, the entire Nodes structure gets cloned when only one nodes text needs to change.
            let mut nodes = model.nodes.clone();

            let highlighted_node = nodes
                .get_mut(&model.highlighted_node)
                .expect("the highlighted node should always exist");

            if highlighted_node.is_in_edit_mode() {
                highlighted_node.update_edit(&pressed);
                HandledInput::NodesChanged(nodes)
            } else {
                HandledInput::NoChange
            }
        }

        Input {
            ctrl_held: true,
            shift_held: false,
            pressed: Some(Pressed::Arrow(direction)),
            ..
        } => {
            let newly_highlighted_node = model.highlighted_node.neighbor(*direction);

            let mut nodes = Cow::Borrowed(&model.nodes);

            if nodes
                .get(&model.highlighted_node)
                .expect("the previously highlighted node should always exist")
                .is_empty()
            {
                nodes.to_mut().remove(&model.highlighted_node);
            }

            // if there isn't already a node there, create an empty one
            if nodes.get(&newly_highlighted_node).is_none() {
                let prev_value = nodes.to_mut().insert(newly_highlighted_node, Node::empty());
                assert!(prev_value.is_none());
            }

            let ghost_nodes =
                GhostNodes::CreateGhosts(determine_ghost_node_locs(&nodes, newly_highlighted_node));

            match nodes {
                Cow::Owned(nodes) => {
                    HandledInput::EverythingChanged(nodes, ghost_nodes, newly_highlighted_node)
                }

                Cow::Borrowed(_) => HandledInput::ViewChanged(ghost_nodes, newly_highlighted_node),
            }
        }

        Input {
            ctrl_held: true,
            shift_held: true,
            pressed: Some(Pressed::Arrow(direction)),
            ..
        } => {
            let target = model.highlighted_node.neighbor(*direction);

            let target_is_empty = !model.nodes.contains_key(&target);
            let highlighted_is_moveable = model
                .nodes
                .get(&model.highlighted_node)
                .is_some_and(Node::is_in_edit_mode);

            if target_is_empty && highlighted_is_moveable {
                let mut nodes = model.nodes.clone();

                let node_to_move = nodes.remove(&model.highlighted_node).unwrap();

                nodes.insert(target, node_to_move);

                let ghost_nodes = GhostNodes::MoveGhosts(determine_ghost_node_locs(&nodes, target));

                HandledInput::EverythingChanged(nodes, ghost_nodes, target)
            } else {
                HandledInput::NoChange
            }
        }

        Input {
            ctrl_held: true,
            shift_held: false,
            pressed: None,
            ..
        } => {
            let ghost_nodes = GhostNodes::CreateGhosts(determine_ghost_node_locs(
                &model.nodes,
                model.highlighted_node,
            ));

            HandledInput::ViewChanged(ghost_nodes, model.highlighted_node)
        }

        Input {
            ctrl_held: true,
            shift_held: true,
            pressed: None,
            ..
        } => {
            let highlighted_is_moveable = model
                .nodes
                .get(&model.highlighted_node)
                .expect("highlighted node should always exist")
                .is_in_edit_mode();

            let ghost_nodes = if highlighted_is_moveable {
                GhostNodes::MoveGhosts(determine_ghost_node_locs(
                    &model.nodes,
                    model.highlighted_node,
                ))
            } else {
                GhostNodes::None
            };

            HandledInput::ViewChanged(ghost_nodes, model.highlighted_node)
        }

        Input {
            ctrl_held: false, ..
        }
        | Input {
            ctrl_held: true,
            pressed: Some(_),
            ..
        } => HandledInput::ViewChanged(GhostNodes::None, model.highlighted_node),
    }
}

fn stop_execution(nodes: &Nodes, starting_node: NodeCoord) -> Option<Nodes> {
    let new_nodes = Nodes::new();

    match seek_nodes(nodes, new_nodes, starting_node, &mut stop_node_execution) {
        Ok(new_nodes) => Some(new_nodes),
        Err(_) => None,
    }
}

fn stop_node_execution(
    old_nodes: &Nodes,
    mut new_nodes: Nodes,
    node_loc: NodeCoord,
) -> Result<Nodes, Nodes> {
    let Some(mut node) = old_nodes.get(&node_loc).cloned() else {
        return Err(new_nodes);
    };

    if node.exec.is_some() {
        node.exec = None;
        new_nodes.insert(node_loc, node);
        Ok(new_nodes)
    } else {
        Err(new_nodes)
    }
}

fn step_execution(nodes: &Nodes, starting_node: NodeCoord) -> Option<Nodes> {
    let new_nodes = Nodes::new();

    match seek_nodes(nodes, new_nodes, starting_node, &mut step_node_execution) {
        Ok(new_nodes) => Some(new_nodes),
        Err(_) => None,
    }
}

fn seek_nodes(
    old_nodes: &Nodes,
    mut new_nodes: Nodes,
    start_loc: NodeCoord,
    transform: &mut impl FnMut(&Nodes, Nodes, NodeCoord) -> Result<Nodes, Nodes>,
) -> Result<Nodes, Nodes> {
    if new_nodes.contains_key(&start_loc) {
        return Ok(new_nodes);
    }

    new_nodes = transform(old_nodes, new_nodes, start_loc)?;

    for neighbor_dir in Dir::ALL {
        let neighbor_loc = start_loc.neighbor(neighbor_dir);

        new_nodes = match seek_nodes(old_nodes, new_nodes, neighbor_loc, transform) {
            Ok(nodes) => nodes,
            Err(nodes) => nodes,
        }
    }

    Ok(new_nodes)
}

fn step_node_execution(
    old_nodes: &Nodes,
    mut new_nodes: Nodes,
    node_loc: NodeCoord,
) -> Result<Nodes, Nodes> {
    let Some(mut node) = old_nodes.get(&node_loc).cloned() else {
        return Err(new_nodes);
    };

    let Some(ref mut exec) = node.exec else {
        if let Ok(exec) = NodeExec::init(&node.text)
            && !exec.code.is_empty()
        {
            node.exec = Some(exec);
            new_nodes.insert(node_loc, node);
            return Ok(new_nodes);
        } else {
            new_nodes.insert(node_loc, node);
            return Err(new_nodes);
        }
    };

    if let NodeIO::Outbound(_, _) = exec.io {
        return Ok(new_nodes);
    }

    match exec.code[exec.ip as usize].op {
        Op::Mov(src, dst) => {
            if let Some(value) = get_src_value(exec, node_loc, old_nodes, &mut new_nodes, src) {
                match dst {
                    Dst::Acc => {
                        exec.acc = value;
                        exec.inc_ip();
                    }
                    Dst::Dir(target_dir) => exec.io = NodeIO::Outbound(target_dir, value),
                    Dst::Nil => exec.inc_ip(),
                }
            }
        }
        Op::Nop => exec.inc_ip(),
        Op::Swp => {
            (exec.acc, exec.bak) = (exec.bak, exec.acc);
            exec.inc_ip();
        }
        Op::Sav => {
            exec.bak = exec.acc;
            exec.inc_ip();
        }
        Op::Add(src) => {
            if let Some(value) = get_src_value(exec, node_loc, old_nodes, &mut new_nodes, src) {
                exec.acc = exec.acc.saturating_add(value);
                exec.inc_ip();
            }
        }
        Op::Sub(src) => {
            if let Some(value) = get_src_value(exec, node_loc, old_nodes, &mut new_nodes, src) {
                exec.acc = exec.acc.saturating_sub(value);
                exec.inc_ip();
            }
        }
        Op::Neg => {
            exec.acc = -exec.acc;
            exec.inc_ip();
        }
        Op::Jmp(target) => exec.ip = target,
        Op::Jez(target) => {
            if exec.acc == 0 {
                exec.ip = target
            } else {
                exec.inc_ip();
            }
        }
        Op::Jnz(target) => {
            if exec.acc != 0 {
                exec.ip = target
            } else {
                exec.inc_ip();
            }
        }
        Op::Jgz(target) => {
            if exec.acc > 0 {
                exec.ip = target
            } else {
                exec.inc_ip();
            }
        }
        Op::Jlz(target) => {
            if exec.acc < 0 {
                exec.ip = target
            } else {
                exec.inc_ip();
            }
        }
        Op::Jro(src) => {
            if let Some(value) = get_src_value(exec, node_loc, old_nodes, &mut new_nodes, src) {
                exec.jro(value);
            }
        }
    }

    let _ = new_nodes.try_insert(node_loc, node);

    Ok(new_nodes)
}

fn get_src_value(
    exec: &mut NodeExec,
    node_loc: NodeCoord,
    old_nodes: &Nodes,
    new_nodes: &mut Nodes,
    src: Src,
) -> Option<Num> {
    match src {
        Src::Imm(num) => Some(num),
        Src::Acc => Some(exec.acc),
        Src::Dir(target_dir) => {
            exec.io = NodeIO::Inbound(target_dir);

            let neighbor_loc = node_loc.neighbor(target_dir);
            let mut neighbor = old_nodes.get(&neighbor_loc)?.clone();
            let neighbor_exec = neighbor.exec.as_mut()?;

            if let NodeIO::Outbound(neighbor_outbound_dir, value) = neighbor_exec.io
                && neighbor_outbound_dir == target_dir.inverse()
            {
                neighbor_exec.inc_ip();

                neighbor_exec.io = NodeIO::None;
                exec.io = NodeIO::None;

                new_nodes.insert(neighbor_loc, neighbor);

                Some(value)
            } else {
                None
            }
        }
        Src::Nil => Some(0),
    }
}

fn update_camera(
    camera: Camera2D,
    highlighted_node: NodeCoord,
    window_dimensions: (i32, i32),
    mouse_wheel_move: f32,
) -> Camera2D {
    let target =
        camera.target + ((highlighted_node.center() - camera.target) * 0.7).clamp(-200.0..200.0);

    let zoom = (camera.zoom + mouse_wheel_move * 0.2).clamp(0.5, 4.0);

    let offset = Vector2 {
        x: window_dimensions.0 as f32 / 2.,
        y: window_dimensions.1 as f32 / 2.,
    };

    Camera2D {
        target,
        zoom,
        offset,
        ..camera
    }
}

fn determine_ghost_node_locs(nodes: &Nodes, highlighted_node: NodeCoord) -> GhostLocs {
    let NodeCoord { x, y } = highlighted_node;

    let adjacent_nodes = [
        NodeCoord::at(x, y - 1), // up
        NodeCoord::at(x, y + 1), // down
        NodeCoord::at(x - 1, y), // left
        NodeCoord::at(x + 1, y), // right
    ];

    adjacent_nodes.map(|coord| {
        if nodes.contains_key(&coord) {
            None
        } else {
            Some(coord)
        }
    })
}

type Num = i8;

type NodeCode<Label = u8> = ArrayVec<Instruction<Label>, NODE_LINES>;

#[derive(Clone)]
struct NodeExec {
    acc: Num,
    bak: Num,
    code: NodeCode,
    io: NodeIO,
    ip: u8,
}

#[derive(PartialEq, Eq, Clone)]
enum NodeIO {
    None,
    Outbound(Dir, Num),
    Inbound(Dir),
}

impl NodeExec {
    fn init(node_text: &NodeText) -> Result<Self, ParseErr> {
        let code = parse_node_text(node_text)?;

        Ok(Self {
            acc: 0,
            bak: 0,
            code,
            io: NodeIO::None,
            ip: 0,
        })
    }

    fn inc_ip(&mut self) {
        self.ip += 1;
        if self.ip as usize >= self.code.len() {
            self.ip = 0;
        }
    }

    fn jro(&mut self, offset: Num) {
        if offset < 0 {
            self.ip = self.ip.saturating_sub(offset.abs() as u8);
        } else {
            self.ip = self.ip.saturating_add(offset as u8);
            if self.ip as usize >= self.code.len() {
                self.ip = (self.code.len() - 1) as u8;
            }
        }
    }
}

#[derive(Clone, Copy)]
struct Instruction<Label: Copy = u8> {
    op: Op<Label>,
    src_line: u8,
}

#[derive(Clone, Copy)]
enum Op<Label: Copy> {
    Mov(Src, Dst),
    Nop,
    Swp,
    Sav,
    Add(Src),
    Sub(Src),
    Neg,
    Jmp(Label),
    Jez(Label),
    Jnz(Label),
    Jgz(Label),
    Jlz(Label),
    Jro(Src),
}

#[derive(Clone, Copy)]
enum Src {
    Imm(Num),
    Dir(Dir),
    Acc,
    Nil,
}

#[derive(Clone, Copy)]
enum Dst {
    Dir(Dir),
    Acc,
    Nil,
}

#[derive(Clone)]
struct ParseErr {
    problem: ParseProblem,
    line: u8,
}

#[derive(Clone)]
enum ParseProblem {
    NotEnoughArgs,
    TooManyArgs,
    InvalidSrc,
    InvalidDst,
    InvalidInstruction,
    UndefinedLabel,
}

impl ParseProblem {
    fn to_str(&self) -> &'static str {
        match self {
            ParseProblem::NotEnoughArgs => "NOT ENOUGH ARGS",
            ParseProblem::TooManyArgs => "TOO MANY ARGS",
            ParseProblem::InvalidSrc => "INVALID SOURCE ARG",
            ParseProblem::InvalidDst => "INVALID DESTINATION ARG",
            ParseProblem::InvalidInstruction => "INVALID OPCODE",
            ParseProblem::UndefinedLabel => "UNDEFINED LABEL",
        }
    }
}

fn parse_node_text(node_text: &NodeText) -> Result<NodeCode, ParseErr> {
    let mut code = NodeCode::<&str>::new();

    // maps labels to instruction indices
    let mut labels: HashMap<&str, u8> = HashMap::new();

    for (line_no, full_line) in node_text.split('\n').enumerate() {
        let Some(semantic_text) = full_line.split('#').next() else {
            continue;
        };

        let op_text = match semantic_text.split_once(':') {
            Some((label, rest)) => {
                // label refers to the next instruction to be pushed to the list of instructions
                let label_dest = code.len();
                labels.insert(label, label_dest as u8);
                rest
            }
            None => semantic_text,
        };

        let tokens = &mut op_text.split_ascii_whitespace();

        let Some(opcode) = tokens.next() else {
            continue;
        };

        let line_no = line_no as u8;

        let op = match opcode {
            "MOV" => Op::Mov(expect_src(tokens, line_no)?, expect_dst(tokens, line_no)?),
            "NOP" => Op::Nop,
            "SWP" => Op::Swp,
            "SAV" => Op::Sav,
            "ADD" => Op::Add(expect_src(tokens, line_no)?),
            "SUB" => Op::Sub(expect_src(tokens, line_no)?),
            "NEG" => Op::Neg,
            "JMP" => Op::Jmp(expect_label(tokens, line_no)?),
            "JEZ" => Op::Jez(expect_label(tokens, line_no)?),
            "JNZ" => Op::Jnz(expect_label(tokens, line_no)?),
            "JGZ" => Op::Jgz(expect_label(tokens, line_no)?),
            "JLZ" => Op::Jlz(expect_label(tokens, line_no)?),
            "JRO" => Op::Jro(expect_src(tokens, line_no)?),

            _ => {
                return Err(ParseErr {
                    problem: ParseProblem::InvalidInstruction,
                    line: line_no,
                });
            }
        };

        if tokens.next().is_some() {
            return Err(ParseErr {
                problem: ParseProblem::TooManyArgs,
                line: line_no,
            });
        }

        code.push(Instruction {
            op,
            src_line: line_no,
        });
    }

    code.into_iter()
        .map(|instr| {
            let resolve = |label: &str| {
                labels.get(&label).copied().ok_or(ParseErr {
                    problem: ParseProblem::UndefinedLabel,
                    line: instr.src_line,
                })
            };

            let op = match instr.op {
                Op::Mov(src, dst) => Op::Mov(src, dst),
                Op::Nop => Op::Nop,
                Op::Swp => Op::Swp,
                Op::Sav => Op::Sav,
                Op::Add(src) => Op::Add(src),
                Op::Sub(src) => Op::Sub(src),
                Op::Neg => Op::Neg,
                Op::Jmp(label) => Op::Jmp(resolve(label)?),
                Op::Jez(label) => Op::Jez(resolve(label)?),
                Op::Jnz(label) => Op::Jnz(resolve(label)?),
                Op::Jgz(label) => Op::Jgz(resolve(label)?),
                Op::Jlz(label) => Op::Jlz(resolve(label)?),
                Op::Jro(src) => Op::Jro(src),
            };

            Ok(Instruction {
                op,
                src_line: instr.src_line,
            })
        })
        .try_collect()
}

fn expect_label<'txt>(
    tokens: &mut impl Iterator<Item = &'txt str>,
    line: u8,
) -> Result<&'txt str, ParseErr> {
    let Some(label) = tokens.next() else {
        return Err(ParseErr {
            problem: ParseProblem::NotEnoughArgs,
            line,
        });
    };

    Ok(label)
}

fn expect_src<'txt>(
    tokens: &mut impl Iterator<Item = &'txt str>,
    line: u8,
) -> Result<Src, ParseErr> {
    let Some(arg) = tokens.next() else {
        return Err(ParseErr {
            problem: ParseProblem::NotEnoughArgs,
            line,
        });
    };

    match arg {
        "ACC" => Ok(Src::Acc),
        "UP" => Ok(Src::Dir(Dir::Up)),
        "DOWN" => Ok(Src::Dir(Dir::Down)),
        "LEFT" => Ok(Src::Dir(Dir::Left)),
        "RIGHT" => Ok(Src::Dir(Dir::Right)),
        "NIL" => Ok(Src::Nil),
        other => {
            if let Ok(num) = other.parse() {
                Ok(Src::Imm(num))
            } else {
                Err(ParseErr {
                    problem: ParseProblem::InvalidSrc,
                    line,
                })
            }
        }
    }
}

fn expect_dst<'txt>(
    tokens: &mut impl Iterator<Item = &'txt str>,
    line: u8,
) -> Result<Dst, ParseErr> {
    let Some(arg) = tokens.next() else {
        return Err(ParseErr {
            problem: ParseProblem::NotEnoughArgs,
            line,
        });
    };

    match arg {
        "ACC" => Ok(Dst::Acc),
        "UP" => Ok(Dst::Dir(Dir::Up)),
        "DOWN" => Ok(Dst::Dir(Dir::Down)),
        "LEFT" => Ok(Dst::Dir(Dir::Left)),
        "RIGHT" => Ok(Dst::Dir(Dir::Right)),
        "NIL" => Ok(Dst::Nil),
        _ => Err(ParseErr {
            problem: ParseProblem::InvalidDst,
            line,
        }),
    }
}
