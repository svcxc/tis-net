#![feature(array_repeat)]
#![feature(let_chains)]
#![feature(map_try_insert)]
#![feature(iterator_try_collect)]
#![feature(map_many_mut)]
#![feature(iter_intersperse)]

use std::{collections::HashMap, f32, fmt::Debug};

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

const GHOST_COLOR: Color = Color::GRAY;

type Nodes = HashMap<NodeCoord, Node>;

struct Model {
    camera: Camera2D,
    nodes: Nodes,
    highlighted_node: NodeCoord,
    ghosts: Ghosts,
}

enum Ghosts {
    MoveNodeGhosts,
    MoveViewGhosts,
    None,
}

type NodeText = ArrayString<NODE_TEXT_BUFFER_SIZE>;

#[derive(Clone, Debug)]
enum Node {
    Exec(ExecNode),
    // Stack,
}

impl Node {
    fn empty_exec() -> Self {
        Self::Exec(ExecNode::empty())
    }

    fn exec_with_text(text: &str) -> Self {
        Self::Exec(ExecNode::with_text(text))
    }

    fn is_inert(&self) -> bool {
        match self {
            Node::Exec(exec_node) => exec_node.is_in_edit_mode(),
        }
    }
}

#[derive(Clone, Debug)]
struct ExecNode {
    text: NodeText,
    cursor: usize,
    error: Option<ParseErr>,
    exec: Option<NodeExec>,
}

impl ExecNode {
    fn empty() -> Self {
        Self {
            text: ArrayString::new(),
            cursor: 0,
            error: None,
            exec: None,
        }
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
            Pressed::Tab => {} // TODO: TAB, ESC, and DELETE are the only buttons here that can't be used in editing; move them out of this enum in the future
            Pressed::Esc => {}
            Pressed::Delete => {}
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
        self.text_loc() + Vector2::new(0., line_number as f32 * NODE_LINE_HEIGHT)
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
    let (mut rl, thread) = raylib::init().resizable().title("TIS-NET").build();

    rl.set_target_fps(60);
    rl.set_text_line_spacing(NODE_LINE_HEIGHT as _);
    rl.set_exit_key(None);

    let font = rl
        .load_font_from_memory(
            &thread,
            ".ttf",
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

    fn combine<'str>(lines: impl IntoIterator<Item = &'str str>) -> String {
        lines.into_iter().intersperse("\n").collect()
    }

    let left = Node::exec_with_text(&combine([
        "MOV 10 RIGHT",
        "MOV 1 RIGHT",
        "MOV 100 RIGHT",
        "MOV 0 RIGHT",
        "JRO 0",
    ]));

    let middle = Node::exec_with_text(&combine([
        "RST:MOV LEFT ACC",
        "JNZ SK1",
        "MOV 1 RIGHT",
        "JMP RST",
        "SK1:MOV 4 RIGHT",
        "MOV ACC RIGHT",
        "MOV ACC RIGHT",
    ]));

    let right = Node::exec_with_text(&combine([
        "RST:JRO LEFT",
        "MOV ACC RIGHT#1",
        "MOV 0 ACC",
        "JMP RST",
        "SAV#4",
        "SUB LEFT",
        "JGZ SK1",
        "MOV LEFT ACC",
        "JMP RST",
        "SK1:MOV LEFT NIL",
        "SWP",
    ]));

    let nodes = HashMap::from([
        (highlighted_node.neighbor(Dir::Left), left),
        (highlighted_node, middle),
        (highlighted_node.neighbor(Dir::Right), right),
    ]);

    Model {
        camera,
        nodes,
        highlighted_node,
        ghosts: Ghosts::None,
    }
}

fn render(rl: &mut RaylibHandle, thread: &RaylibThread, model: &Model, font: &Font) {
    let mut d = rl.begin_drawing(&thread);
    let mut d = d.begin_mode2D(model.camera);
    let d = &mut d;

    d.clear_background(Color::BLACK);

    render_nodes(d, model, font);

    render_ghosts(d, model);

    let highlighted = model.nodes.get(&model.highlighted_node);

    if let Some(Node::Exec(exec_node)) = highlighted {
        if exec_node.is_in_edit_mode() {
            render_cursor(d, model.highlighted_node, exec_node);
        }
    } else {
        render_dashed_node_border(d, model.highlighted_node, Color::GRAY);
        // render_plus(d, model.highlighted_node.center(), Color::GRAY);
    }
}

fn render_ghosts(d: &mut impl RaylibDraw, model: &Model) {
    match model.ghosts {
        Ghosts::MoveViewGhosts => {
            for dir in Dir::ALL {
                let neighbor_loc = model.highlighted_node.neighbor(dir);
                if !model.nodes.contains_key(&neighbor_loc) {
                    render_dashed_node_border(d, neighbor_loc, GHOST_COLOR);

                    render_arrow(d, neighbor_loc.center(), dir, GHOST_COLOR);
                }
            }
        }

        Ghosts::MoveNodeGhosts => {
            for dir in Dir::ALL {
                let neighbor_loc = model.highlighted_node.neighbor(dir);
                if !model.nodes.contains_key(&neighbor_loc) {
                    render_dashed_node_border(d, neighbor_loc, GHOST_COLOR);

                    render_double_arrow(d, neighbor_loc.center(), dir, GHOST_COLOR);
                }
            }
        }

        Ghosts::None => {}
    }
}

fn render_nodes(d: &mut impl RaylibDraw, model: &Model, font: &Font) {
    for (node_loc, node) in model.nodes.iter() {
        let line_color = if node_loc == &model.highlighted_node {
            Color::WHITE
        } else {
            Color::GRAY
        };

        match node {
            Node::Exec(exec_node) => {
                render_node_border(d, *node_loc, line_color);

                render_node_gizmos(d, *node_loc, &exec_node.exec, font, line_color, Color::GRAY);

                render_node_text(d, exec_node, node_loc, font);

                // the below two things should not be true at the same time if I did my homework
                // (because a node with an error should not be able to begin executing)
                // but this isn't reflected in the type system. If it were to happen though, it means there's a bug
                debug_assert!(!(exec_node.error.is_some() && exec_node.exec.is_some()));

                if let Some(error) = &exec_node.error
                    && show_error(node_loc, exec_node, &model.highlighted_node, error.line)
                {
                    render_error_squiggle(d, *node_loc, &exec_node.text, error.line);
                }

                if let Some(exec) = &exec_node.exec
                    && !exec.code.is_empty()
                {
                    if let NodeIO::Outbound(dir, value) = exec.io {
                        render_io_arrow(d, node_loc, dir, &value.to_string(), font);
                    } else if let NodeIO::Inbound(io_dir) = exec.io
                        && !neighbor_sending_io(&model.nodes, node_loc, io_dir)
                    {
                        render_io_arrow(d, &node_loc.neighbor(io_dir), io_dir.inverse(), "?", font);
                    }
                }
            }
        }
    }

    // error boxes are rendered in a second pass because they need to be rendered over top of everything else
    for (node_loc, node) in model.nodes.iter() {
        if let Node::Exec(
            exec_node @ ExecNode {
                error: Some(error), ..
            },
        ) = &node
            && show_error(node_loc, exec_node, &model.highlighted_node, error.line)
        {
            render_error_msg(d, node_loc, &error.problem, font);
        };
    }
}

fn render_node_text(d: &mut impl RaylibDraw, node: &ExecNode, node_loc: &NodeCoord, font: &Font) {
    let highlight = node.exec.as_ref().and_then(|exec| {
        if exec.code.is_empty() {
            None
        } else {
            Some((
                exec.code[exec.ip as usize].src_line,
                exec.io == NodeIO::None,
            ))
        }
    });

    for (line_no, line) in node.text.split('\n').enumerate() {
        let line_loc = node_loc.line_pos(line_no);

        let line_no = line_no as u8;
        let highlight_mode = match highlight {
            Some((highlight_line, true)) if highlight_line == line_no => Highlight::Executing,
            Some((highlight_line, false)) if highlight_line == line_no => Highlight::IO,
            _ => Highlight::None,
        };

        render_node_text_line(d, line_loc, line, highlight_mode, font);
    }
}

#[derive(PartialEq, Eq, Debug)]
enum Highlight {
    None,
    Executing,
    IO,
}

fn render_node_text_line(
    d: &mut impl RaylibDraw,
    line_loc: Vector2,
    text: &str,
    highlight_mode: Highlight,
    font: &Font,
) {
    let (comment_color, text_color) = if highlight_mode == Highlight::None {
        (Color::GRAY, Color::WHITE)
    } else {
        (Color::BLACK, Color::BLACK)
    };

    if highlight_mode != Highlight::None {
        let highlight_color = if highlight_mode == Highlight::Executing {
            Color::WHITE
        } else {
            Color::GRAY
        };

        let highlight_pos = line_loc
            - Vector2 {
                x: NODE_INSIDE_PADDING * 0.25,
                y: 0.0,
            };

        const HIGHLIGHT_SIZE: Vector2 = Vector2 {
            x: NODE_TEXT_BOX_WIDTH + NODE_INSIDE_PADDING * 0.5,
            y: NODE_LINE_HEIGHT,
        };

        d.draw_rectangle_v(highlight_pos, HIGHLIGHT_SIZE, highlight_color);
    }

    if let Some(comment_start) = text.find('#') {
        let char_offset = Vector2::new(NODE_CHAR_WIDTH, 0.0);
        let comment_offset = char_offset.scale_by(comment_start as f32);

        d.draw_text_ex(
            font,
            &text[..comment_start],
            line_loc,
            NODE_FONT_SIZE,
            NODE_FONT_SPACING,
            text_color,
        );
        d.draw_text_ex(
            font,
            &text[comment_start..],
            line_loc + comment_offset,
            NODE_FONT_SIZE,
            NODE_FONT_SPACING,
            comment_color,
        );
    } else {
        d.draw_text_ex(
            font,
            text,
            line_loc,
            NODE_FONT_SIZE,
            NODE_FONT_SPACING,
            text_color,
        );
    }
}

fn show_error(
    node_loc: &NodeCoord,
    node: &ExecNode,
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
    let Some(Node::Exec(neighbor)) = nodes.get(&node_loc.neighbor(io_dir)) else {
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

fn render_cursor(d: &mut impl RaylibDraw, node_loc: NodeCoord, node: &ExecNode) {
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

fn render_double_arrow(d: &mut impl RaylibDraw, center: Vector2, direction: Dir, color: Color) {
    let dir_vec = direction.normalized();

    let half_arrow_stem = dir_vec.scale_by(NODE_LINE_HEIGHT);

    let arrow_tip = center + half_arrow_stem;
    let arrow_base = center - half_arrow_stem;

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

    d.draw_line_ex(
        arrow_tip,
        arrow_left_wing + half_arrow_stem,
        LINE_THICKNESS,
        color,
    );
    d.draw_line_ex(
        arrow_tip,
        arrow_right_wing + half_arrow_stem,
        LINE_THICKNESS,
        color,
    );
}

fn render_plus(d: &mut impl RaylibDraw, center: Vector2, color: Color) {
    d.draw_line_ex(
        center + Vector2::new(-NODE_LINE_HEIGHT, 0.0),
        center + Vector2::new(NODE_LINE_HEIGHT, 0.0),
        LINE_THICKNESS,
        color,
    );
    d.draw_line_ex(
        center + Vector2::new(0.0, -NODE_LINE_HEIGHT),
        center + Vector2::new(0.0, NODE_LINE_HEIGHT),
        LINE_THICKNESS,
        color,
    );
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
    Delete,
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
    } else if rl.is_key_pressed(KeyboardKey::KEY_DELETE) {
        Some(Pressed::Delete)
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
    let (nodes, highlighted_node, ghosts) = match handle_input(&model, &input) {
        HandledInput::Exit => return None,
        HandledInput::Changes {
            highlighted,
            nodes,
            ghosts,
        } => (
            nodes.unwrap_or(model.nodes),
            highlighted.unwrap_or(model.highlighted_node),
            ghosts,
        ),
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
        highlighted_node,
        ghosts,
    })
}

enum HandledInput {
    Exit,
    Changes {
        highlighted: Option<NodeCoord>,
        nodes: Option<Nodes>,
        ghosts: Ghosts,
    },
}

impl HandledInput {
    fn no_changes(ghosts: Ghosts) -> Self {
        Self::Changes {
            highlighted: None,
            nodes: None,
            ghosts,
        }
    }
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

                HandledInput::Changes {
                    highlighted: None,
                    nodes: Some(nodes),
                    ghosts: Ghosts::None,
                }
            } else {
                HandledInput::Exit
            }
        }

        Input {
            pressed: Some(Pressed::Delete),
            ..
        } => {
            if let Some(node) = model.nodes.get(&model.highlighted_node)
                && node.is_inert()
            {
                let mut nodes = model.nodes.clone();

                nodes.remove(&model.highlighted_node);

                HandledInput::Changes {
                    highlighted: None,
                    nodes: Some(nodes),
                    ghosts: Ghosts::None,
                }
            } else {
                HandledInput::no_changes(Ghosts::None)
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

                HandledInput::Changes {
                    highlighted: None,
                    nodes: Some(nodes),
                    ghosts: Ghosts::None,
                }
            } else {
                HandledInput::no_changes(Ghosts::None)
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

            let highlighted_node = model.nodes.get(&model.highlighted_node);

            if let Some(Node::Exec(exec_node)) = highlighted_node
                && exec_node.is_in_edit_mode()
            {
                let mut nodes = model.nodes.clone();

                let mut exec_node = exec_node.clone();

                exec_node.update_edit(&pressed);

                nodes.insert(model.highlighted_node, Node::Exec(exec_node));

                HandledInput::Changes {
                    highlighted: None,
                    nodes: Some(nodes),
                    ghosts: Ghosts::None,
                }
            } else if highlighted_node.is_none() && pressed == &Pressed::Char('E') {
                let mut nodes = model.nodes.clone();

                nodes.insert(model.highlighted_node, Node::empty_exec());

                HandledInput::Changes {
                    highlighted: None,
                    nodes: Some(nodes),
                    ghosts: Ghosts::None,
                }
            } else {
                HandledInput::no_changes(Ghosts::None)
            }
        }

        Input {
            ctrl_held: true,
            shift_held: false,
            pressed: Some(Pressed::Arrow(direction)),
            ..
        } => HandledInput::Changes {
            highlighted: Some(model.highlighted_node.neighbor(*direction)),
            nodes: None,
            ghosts: Ghosts::MoveViewGhosts,
        },

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
                .is_some_and(Node::is_inert);

            if target_is_empty && highlighted_is_moveable {
                let mut nodes = model.nodes.clone();

                let node_to_move = nodes.remove(&model.highlighted_node).unwrap();

                nodes.insert(target, node_to_move);

                HandledInput::Changes {
                    highlighted: Some(target),
                    nodes: Some(nodes),
                    ghosts: Ghosts::MoveNodeGhosts,
                }
            } else {
                HandledInput::no_changes(Ghosts::None)
            }
        }

        Input {
            ctrl_held: true,
            shift_held: false,
            pressed: None,
            ..
        } => HandledInput::no_changes(Ghosts::MoveViewGhosts),

        Input {
            ctrl_held: true,
            shift_held: true,
            pressed: None,
            ..
        } => {
            let highlighted_is_moveable = model
                .nodes
                .get(&model.highlighted_node)
                .is_some_and(Node::is_inert);

            let ghosts = if highlighted_is_moveable {
                Ghosts::MoveNodeGhosts
            } else {
                Ghosts::None
            };

            HandledInput::no_changes(ghosts)
        }

        Input {
            ctrl_held: false, ..
        }
        | Input {
            ctrl_held: true,
            pressed: Some(_),
            ..
        } => HandledInput::no_changes(Ghosts::None),
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

    if new_nodes.contains_key(&node_loc) {
        return Err(new_nodes);
    }

    match &mut node {
        Node::Exec(exec_node) => {
            if exec_node.exec.is_some() {
                exec_node.exec = None;
                new_nodes.insert(node_loc, node);
                Ok(new_nodes)
            } else {
                Err(new_nodes)
            }
        }
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

    if new_nodes.contains_key(&node_loc) {
        return Err(new_nodes);
    }

    match &mut node {
        Node::Exec(exec_node) => {
            let Some(ref mut exec) = exec_node.exec else {
                if let Ok(exec) = NodeExec::init(&exec_node.text)
                    && !exec.code.is_empty()
                {
                    exec_node.exec = Some(exec);
                    new_nodes.insert(node_loc, node);
                    return Ok(new_nodes);
                } else {
                    new_nodes.insert(node_loc, node);
                    return Err(new_nodes);
                }
            };

            if let NodeIO::Outbound(_, _) = exec.io {
                new_nodes.try_insert(node_loc, node).unwrap();
                return Ok(new_nodes);
            }

            match exec.code[exec.ip as usize].op {
                Op::Mov(src, dst) => {
                    if let Some(value) =
                        get_src_value(exec, node_loc, old_nodes, &mut new_nodes, src)
                    {
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
                    if let Some(value) =
                        get_src_value(exec, node_loc, old_nodes, &mut new_nodes, src)
                    {
                        exec.acc = exec.acc.saturating_add(value);
                        exec.inc_ip();
                    }
                }
                Op::Sub(src) => {
                    if let Some(value) =
                        get_src_value(exec, node_loc, old_nodes, &mut new_nodes, src)
                    {
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
                    if let Some(value) =
                        get_src_value(exec, node_loc, old_nodes, &mut new_nodes, src)
                    {
                        exec.jro(value);
                    }
                }
            }

            new_nodes.try_insert(node_loc, node).unwrap();

            Ok(new_nodes)
        }
    }
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
            let neighbor = old_nodes.get(&neighbor_loc)?;

            match neighbor {
                Node::Exec(exec_node) => {
                    let mut neighbor = exec_node.clone();
                    let neighbor_exec = neighbor.exec.as_mut()?;

                    if let NodeIO::Outbound(neighbor_outbound_dir, value) = neighbor_exec.io
                        && neighbor_outbound_dir == target_dir.inverse()
                    {
                        neighbor_exec.inc_ip();

                        neighbor_exec.io = NodeIO::None;
                        exec.io = NodeIO::None;

                        new_nodes.insert(neighbor_loc, Node::Exec(neighbor));

                        Some(value)
                    } else {
                        None
                    }
                }
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

type Num = i8;

type NodeCode<Label = u8> = ArrayVec<Instruction<Label>, NODE_LINES>;

#[derive(Clone, Debug)]
struct NodeExec {
    acc: Num,
    bak: Num,
    code: NodeCode,
    io: NodeIO,
    ip: u8,
}

#[derive(PartialEq, Eq, Clone, Debug)]
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

#[derive(Clone, Copy, Debug)]
struct Instruction<Label: Debug + Copy = u8> {
    op: Op<Label>,
    src_line: u8,
}

#[derive(Clone, Copy, Debug)]
enum Op<Label: Debug + Copy> {
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

#[derive(Clone, Copy, Debug)]
enum Src {
    Imm(Num),
    Dir(Dir),
    Acc,
    Nil,
}

#[derive(Clone, Copy, Debug)]
enum Dst {
    Dir(Dir),
    Acc,
    Nil,
}

#[derive(Clone, Debug)]
struct ParseErr {
    problem: ParseProblem,
    line: u8,
}

#[derive(Clone, Debug)]
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
