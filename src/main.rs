#![feature(let_chains)]
#![feature(map_try_insert)]
#![feature(iterator_try_collect)]
#![feature(iter_intersperse)]

mod consts;
mod dir;
mod exec_node;
mod input_node;
mod num;

use crate::dir::Dir;
use crate::exec_node::{ExecNode, ExecNodeState, ParseErr, ParseProblem};
use crate::input_node::InputNode;

use std::{
    cmp::Ordering,
    collections::{HashMap, hash_map::Entry},
    f32,
    fmt::Debug,
};

use arrayvec::{ArrayString, ArrayVec};
use raylib::prelude::*;
use sorted_vec::SortedSet;

type Nodes = HashMap<NodeCoord, Node>;

struct State {
    camera: Camera2D,
    model: Model,
}

struct Model {
    nodes: Nodes,
    highlighted_node: NodeCoord,
    ghosts: Ghosts,
    node_clipboard: Option<Node>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Modifiers {
    None,
    Ctrl,
    Shift,
    CtrlShift,
}

#[derive(PartialEq, Eq)]
enum Ghosts {
    MoveView,
    MoveNode,
    None,
}

#[derive(Clone, Debug)]
struct Node {
    variant: NodeType,
    outbox: NodeOutbox,
}

#[derive(Clone, Debug)]
enum NodeType {
    Exec(ExecNode),
    Input(InputNode),
}

#[derive(Clone, Debug)]
enum NodeOutbox {
    Empty,
    Directional(Dir, Num),
    Any(Num),
}

impl Node {
    fn empty_exec() -> Self {
        Node {
            variant: NodeType::Exec(ExecNode::empty()),
            outbox: NodeOutbox::Empty,
        }
    }

    fn exec_with_text(text: &str) -> Option<Self> {
        Some(Node {
            variant: NodeType::Exec(ExecNode::with_text(text)?),
            outbox: NodeOutbox::Empty,
        })
    }

    fn exec_with_lines<'str>(lines: impl IntoIterator<Item = &'str str>) -> Option<Self> {
        let text: String = lines.into_iter().intersperse("\n").collect();

        Self::exec_with_text(&text)
    }

    fn empty_input() -> Self {
        Node {
            variant: NodeType::Input(InputNode::empty()),
            outbox: NodeOutbox::Empty,
        }
    }

    fn input_with_data(data: ArrayVec<Num, { input_node::INPUT_NODE_CAP }>) -> Self {
        Node {
            variant: NodeType::Input(InputNode::with_data(data)),
            outbox: NodeOutbox::Empty,
        }
    }
}

#[derive(PartialEq, Eq, Hash, Clone, Copy, Debug)]
struct NodeCoord {
    x: isize,
    y: isize,
}

impl Ord for NodeCoord {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        match self.y.cmp(&other.y) {
            Ordering::Less => Ordering::Less,
            Ordering::Greater => Ordering::Greater,
            Ordering::Equal => self.x.cmp(&other.x),
        }
    }
}

impl PartialOrd for NodeCoord {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
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
        .scale_by(consts::NODE_OUTSIDE_SIDE_LENGTH + consts::NODE_OUTSIDE_PADDING)
    }

    fn top_right_corner(&self) -> Vector2 {
        self.top_left_corner()
            + Vector2 {
                x: consts::NODE_OUTSIDE_SIDE_LENGTH,
                y: 0.,
            }
    }

    fn bottom_left_corner(&self) -> Vector2 {
        self.top_left_corner()
            + Vector2 {
                x: 0.,
                y: consts::NODE_OUTSIDE_SIDE_LENGTH,
            }
    }

    fn bottom_right_corner(&self) -> Vector2 {
        self.top_left_corner()
            + Vector2 {
                x: consts::NODE_OUTSIDE_SIDE_LENGTH,
                y: consts::NODE_OUTSIDE_SIDE_LENGTH,
            }
    }

    fn text_loc(&self) -> Vector2 {
        self.top_left_corner() + Vector2::one().scale_by(consts::NODE_INSIDE_PADDING)
    }

    fn line_pos(&self, line_number: usize) -> Vector2 {
        self.text_loc() + Vector2::new(0., line_number as f32 * consts::NODE_LINE_HEIGHT)
    }

    fn char_pos(&self, line: usize, column: usize) -> Vector2 {
        self.text_loc()
            + Vector2::new(
                column as f32 * consts::NODE_CHAR_WIDTH,
                line as f32 * consts::NODE_LINE_HEIGHT,
            )
    }

    fn center(&self) -> Vector2 {
        self.top_left_corner() + Vector2::one().scale_by(consts::NODE_OUTSIDE_SIDE_LENGTH / 2.)
    }

    fn io_indicator(&self, dir: Dir) -> Vector2 {
        self.center()
            + dir
                .normalized()
                .scale_by((consts::NODE_OUTSIDE_SIDE_LENGTH + consts::NODE_OUTSIDE_PADDING) / 2.0)
            + dir
                .rotate_right()
                .normalized()
                .scale_by(consts::NODE_OUTSIDE_SIDE_LENGTH / 4.0)
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
    rl.set_text_line_spacing(consts::NODE_LINE_HEIGHT as _);
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

    let mut state = init();
    let mut repeat_key = RepeatKey::None;

    loop {
        if rl.window_should_close() {
            break;
        }

        let input = get_input(&mut rl, &mut repeat_key);

        let output;
        (state, output) = match update(state, input) {
            Update::Exit => break,
            Update::Update { new, output } => (new, output),
        };

        if let Some(copied) = output.clipboard {
            rl.set_clipboard_text(&copied)
                .expect("this shouldn't be possible");
        }

        render(&mut rl, &thread, &state, &font);
    }
}

fn init() -> State {
    let camera = Camera2D {
        offset: Default::default(),
        target: Default::default(),
        rotation: Default::default(),
        zoom: 0.85,
    };

    let (nodes, highlighted_node) = parse_toml(include_str!("default.toml")).unwrap();

    State {
        camera,
        model: Model {
            nodes,
            highlighted_node,
            ghosts: Ghosts::None,
            node_clipboard: None,
        },
    }
}

fn render(rl: &mut RaylibHandle, thread: &RaylibThread, state: &State, font: &Font) {
    let mut d = rl.begin_drawing(&thread);
    let mut d = d.begin_mode2D(state.camera);
    let d = &mut d;

    let model = &state.model;

    d.clear_background(Color::BLACK);

    render_nodes(d, model, font);

    render_ghosts(d, model);

    let highlighted = model.nodes.get(&model.highlighted_node);

    match highlighted.map(|Node { variant, .. }| variant) {
        Some(NodeType::Exec(exec_node)) => {
            if exec_node.is_in_edit_mode() {
                render_cursor(d, model.highlighted_node, exec_node);
            }
        }

        Some(NodeType::Input(_)) => {}

        None => {
            render_dashed_node_border(d, model.highlighted_node, Color::GRAY);

            render_plus(d, model.highlighted_node.center(), Color::GRAY);
        }
    }
}

fn render_ghosts(d: &mut impl RaylibDraw, model: &Model) {
    match model.ghosts {
        Ghosts::MoveView => {
            for dir in Dir::ALL {
                let neighbor_loc = model.highlighted_node.neighbor(dir);
                if !model.nodes.contains_key(&neighbor_loc) {
                    render_dashed_node_border(d, neighbor_loc, consts::GHOST_COLOR);

                    render_arrow(d, neighbor_loc.center(), dir, consts::GHOST_COLOR);
                }
            }
        }

        Ghosts::MoveNode => {
            for dir in Dir::ALL {
                let neighbor_loc = model.highlighted_node.neighbor(dir);
                if !model.nodes.contains_key(&neighbor_loc) {
                    render_dashed_node_border(d, neighbor_loc, consts::GHOST_COLOR);

                    render_double_arrow(d, neighbor_loc.center(), dir, consts::GHOST_COLOR);
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

        match &node.variant {
            NodeType::Exec(exec_node) => {
                render_node_border(d, *node_loc, line_color);

                let state = exec_node.state();

                todo!()

                // render_node_gizmos(d, *node_loc, &exec_node.exec, font, line_color, Color::GRAY);

                // render_node_text(d, exec_node, node_loc, font);

                // the below two things should not be true at the same time if I did my homework
                // (because a node with an error should not be able to begin executing)
                // but this isn't reflected in the type system. If it were to happen though, it means there's a bug
                // debug_assert!(!(exec_node.error.is_some() && exec_node.exec.is_some()));

                // if let Some(error) = &exec_node.error
                //     && show_error(node_loc, exec_node, &model.highlighted_node, error.line)
                // {
                //     render_error_squiggle(d, *node_loc, &exec_node.text, error.line);
                // }

                // if let Some(exec) = &exec_node.exec
                //     && !exec.code.is_empty()
                // {
                //     if let NodeIO::Outbound(dir, value) = exec.io {
                //         render_io_arrow(d, node_loc, dir, &value.to_string(), font);
                //     } else if let NodeIO::Inbound(io_dir) = exec.io
                //         && !neighbor_sending_io(&model.nodes, node_loc, io_dir)
                //     {
                //         render_io_arrow(d, &node_loc.neighbor(io_dir), io_dir.inverse(), "?", font);
                //     }
                // }
            }

            NodeType::Input(input_node) => {
                render_node_border(d, *node_loc, line_color);

                let str;
                let label = if let Some(i) = input_node.index {
                    str = i.to_string();
                    &str
                } else {
                    "INPUT NODE"
                };

                render_centered_text(d, label, node_loc.center(), font, Color::WHITE);

                if let Some(num) = input_node.current() {
                    render_io_arrow(d, node_loc, Dir::Down, &num.to_string(), font);
                }
            }
        }
    }

    // error boxes are rendered in a second pass because they need to be rendered over top of everything else
    for (node_loc, node) in model.nodes.iter() {
        if let NodeType::Exec(exec_node) = &node.variant
            && let ExecNodeState::Errored(error) = exec_node.state()
            && show_error(node_loc, exec_node, &model.highlighted_node, error.line)
        {
            render_error_msg(d, node_loc, &error.problem, font);
        };
    }
}

fn render_node_text(d: &mut impl RaylibDraw, node: &ExecNode, node_loc: &NodeCoord, font: &Font) {
    todo!()
    // let highlight = if let Some(ref exec) = node.exec
    //     && let Some(instr) = exec.code.get(exec.ip as usize)
    // {
    //     Highlight::Executing {
    //         line: instr.src_line as usize,
    //         blocked: !matches!(exec.io, NodeIO::None),
    //     }
    // } else if node.text_selected() {
    //     let (start, end) = node.selection_range();

    //     let (start_line, start_col) = line_column(&node.text, start);
    //     let (end_line, end_col) = line_column(&node.text, end);

    //     Highlight::Selected {
    //         start_line,
    //         start_col,
    //         end_line,
    //         end_col,
    //     }
    // } else {
    //     Highlight::None
    // };

    // for (line_no, line_text) in node.text.split('\n').enumerate() {
    //     let line_loc = node_loc.line_pos(line_no);

    //     match highlight {
    //         Highlight::Executing { line, blocked } if line == line_no => {
    //             let highlight_color = if blocked { Color::GRAY } else { Color::WHITE };

    //             let highlight_pos = line_loc
    //                 - Vector2 {
    //                     x: consts::NODE_INSIDE_PADDING * 0.25,
    //                     y: 0.0,
    //                 };

    //             const HIGHLIGHT_SIZE: Vector2 = Vector2 {
    //                 x: NODE_TEXT_BOX_INSIDE_WIDTH + consts::NODE_INSIDE_PADDING * 0.5,
    //                 y: consts::NODE_LINE_HEIGHT,
    //             };

    //             d.draw_rectangle_v(highlight_pos, HIGHLIGHT_SIZE, highlight_color);

    //             d.draw_text_ex(
    //                 font,
    //                 line_text,
    //                 line_loc,
    //                 consts::NODE_FONT_SIZE,
    //                 consts::NODE_FONT_SPACING,
    //                 Color::BLACK,
    //             );
    //         }

    //         Highlight::Selected {
    //             start_line,
    //             start_col,
    //             end_line,
    //             end_col,
    //         } if start_line <= line_no && line_no <= end_line => {
    //             if let Some(comment_start) = line_text.find('#') {
    //                 let char_offset = Vector2::new(consts::NODE_CHAR_WIDTH, 0.0);
    //                 let comment_offset = char_offset.scale_by(comment_start as f32);

    //                 d.draw_text_ex(
    //                     font,
    //                     &line_text[..comment_start],
    //                     line_loc,
    //                     consts::NODE_FONT_SIZE,
    //                     consts::NODE_FONT_SPACING,
    //                     Color::WHITE,
    //                 );
    //                 d.draw_text_ex(
    //                     font,
    //                     &line_text[comment_start..],
    //                     line_loc + comment_offset,
    //                     consts::NODE_FONT_SIZE,
    //                     consts::NODE_FONT_SPACING,
    //                     Color::GRAY,
    //                 );
    //             } else {
    //                 d.draw_text_ex(
    //                     font,
    //                     line_text,
    //                     line_loc,
    //                     consts::NODE_FONT_SIZE,
    //                     consts::NODE_FONT_SPACING,
    //                     Color::WHITE,
    //                 );
    //             }

    //             let selection_start = if start_line == line_no { start_col } else { 0 };

    //             let selection_end = if end_line == line_no {
    //                 end_col
    //             } else {
    //                 line_text.len() + 1
    //             };

    //             let selection_len = selection_end - selection_start;

    //             let select_highlight_pos = node_loc.char_pos(line_no, selection_start);

    //             let selection_box_size = Vector2 {
    //                 x: selection_len as f32 * consts::NODE_CHAR_WIDTH,
    //                 y: consts::NODE_LINE_HEIGHT,
    //             };

    //             d.draw_rectangle_v(select_highlight_pos, selection_box_size, Color::GRAY);

    //             d.draw_text_ex(
    //                 font,
    //                 line_text,
    //                 line_loc,
    //                 consts::NODE_FONT_SIZE,
    //                 consts::NODE_FONT_SPACING,
    //                 Color::WHITE,
    //             );
    //         }

    //         Highlight::None | Highlight::Executing { .. } | Highlight::Selected { .. } => {
    //             if let Some(comment_start) = line_text.find('#') {
    //                 let char_offset = Vector2::new(consts::NODE_CHAR_WIDTH, 0.0);
    //                 let comment_offset = char_offset.scale_by(comment_start as f32);

    //                 d.draw_text_ex(
    //                     font,
    //                     &line_text[..comment_start],
    //                     line_loc,
    //                     consts::NODE_FONT_SIZE,
    //                     consts::NODE_FONT_SPACING,
    //                     Color::WHITE,
    //                 );
    //                 d.draw_text_ex(
    //                     font,
    //                     &line_text[comment_start..],
    //                     line_loc + comment_offset,
    //                     consts::NODE_FONT_SIZE,
    //                     consts::NODE_FONT_SPACING,
    //                     Color::GRAY,
    //                 );
    //             } else {
    //                 d.draw_text_ex(
    //                     font,
    //                     line_text,
    //                     line_loc,
    //                     consts::NODE_FONT_SIZE,
    //                     consts::NODE_FONT_SPACING,
    //                     Color::WHITE,
    //                 );
    //             }
    //         }
    //     }
    // }
}

#[derive(PartialEq, Eq, Debug, Clone, Copy)]
enum Highlight {
    None,
    Executing {
        line: usize,
        blocked: bool,
    },
    Selected {
        start_line: usize,
        start_col: usize,
        end_line: usize,
        end_col: usize,
    },
}

fn show_error(
    node_loc: &NodeCoord,
    node: &ExecNode,
    highlighted_node: &NodeCoord,
    error_line: u8,
) -> bool {
    node_loc != highlighted_node || !node.cursor_at_error_line(error_line)
}

fn render_error_msg(
    d: &mut impl RaylibDraw,
    node_loc: &NodeCoord,
    problem: &ParseProblem,
    font: &Font,
) {
    const BOX_HEIGHT: f32 = consts::NODE_LINE_HEIGHT + 2.0 * consts::NODE_INSIDE_PADDING;

    const BOX_NODE_PADDING: f32 = 0.25 * (consts::NODE_OUTSIDE_PADDING - BOX_HEIGHT);

    let bottom_left = node_loc.top_left_corner() - Vector2::new(0.0, BOX_NODE_PADDING);

    let top_left = bottom_left - Vector2::new(0.0, BOX_HEIGHT);

    let top_right = top_left + Vector2::new(consts::NODE_OUTSIDE_SIDE_LENGTH, 0.0);
    let bottom_right = bottom_left + Vector2::new(consts::NODE_OUTSIDE_SIDE_LENGTH, 0.0);

    let center = top_left + Vector2::new(0.5 * consts::NODE_OUTSIDE_SIDE_LENGTH, 0.5 * BOX_HEIGHT);

    d.draw_rectangle_v(top_left, bottom_right - top_left, Color::BLACK);

    d.draw_line_ex(top_left, top_right, consts::LINE_THICKNESS, Color::RED);
    d.draw_line_ex(top_left, bottom_left, consts::LINE_THICKNESS, Color::RED);
    d.draw_line_ex(
        bottom_left,
        bottom_right,
        consts::LINE_THICKNESS,
        Color::RED,
    );
    d.draw_line_ex(top_right, bottom_right, consts::LINE_THICKNESS, Color::RED);

    render_centered_text(d, problem.to_str(), center, font, Color::RED);
}

fn neighbor_sending_io(nodes: &Nodes, node_loc: &NodeCoord, io_dir: Dir) -> bool {
    let Some(neighbor) = nodes.get(&node_loc.neighbor(io_dir)) else {
        return false;
    };

    match neighbor.outbox {
        NodeOutbox::Empty => false,
        NodeOutbox::Directional(dir, _) => dir == io_dir.inverse(),
        NodeOutbox::Any(_) => true,
    }
}

fn render_node_gizmos(
    d: &mut impl RaylibDraw,
    node_loc: NodeCoord,
    exec: &ExecNodeState,
    font: &Font,
    primary: Color,
    secondary: Color,
) {
    todo!()

    // let (acc_string, bak_string);

    // let (acc, bak, mode) = if let Some(exec) = exec {
    //     acc_string = exec.acc.to_string();

    //     bak_string = if exec.bak < -99 {
    //         exec.bak.to_string()
    //     } else {
    //         format!("({})", exec.bak)
    //     };

    //     let mode_str = match exec.io {
    //         NodeIO::None => "EXEC",
    //         NodeIO::Inbound(_) => "READ",
    //         NodeIO::Outbound(_, _) => "WRTE",
    //     };

    //     (acc_string.as_str(), bak_string.as_str(), mode_str)
    // } else {
    //     ("0", "(0)", "EDIT")
    // };

    // let placeholder_gizmos = [("ACC", acc), ("BAK", bak), ("LAST", "N/A"), ("MODE", mode)];

    // for (i, (top, bottom)) in placeholder_gizmos.into_iter().enumerate() {
    //     let gizmos_top_left = node_loc.top_right_corner()
    //         - Vector2::new(consts::GIZMO_WIDTH, i as f32 * -consts::GIZMO_HEIGHT);

    //     let left_right = Vector2::new(consts::GIZMO_WIDTH, 0.0);
    //     let top_down = Vector2::new(0.0, consts::GIZMO_HEIGHT);

    //     // draws a rectangle out of individual lines
    //     // doing this makes the lines centered, rather than aligned to the outside
    //     d.draw_line_ex(
    //         gizmos_top_left,
    //         gizmos_top_left + left_right,
    //         consts::LINE_THICKNESS,
    //         primary,
    //     );
    //     d.draw_line_ex(
    //         gizmos_top_left,
    //         gizmos_top_left + top_down,
    //         consts::LINE_THICKNESS,
    //         primary,
    //     );
    //     d.draw_line_ex(
    //         gizmos_top_left + left_right,
    //         gizmos_top_left + left_right + top_down,
    //         consts::LINE_THICKNESS,
    //         primary,
    //     );
    //     d.draw_line_ex(
    //         gizmos_top_left + top_down,
    //         gizmos_top_left + top_down + left_right,
    //         consts::LINE_THICKNESS,
    //         primary,
    //     );

    //     let text_center =
    //         gizmos_top_left + Vector2::new(consts::GIZMO_WIDTH / 2., consts::GIZMO_HEIGHT / 2.);
    //     let text_offset = Vector2::new(0.0, consts::NODE_LINE_HEIGHT / 2.0);
    //     let top_text = text_center - text_offset;
    //     let bottom_text = text_center + text_offset;

    //     render_centered_text(d, top, top_text, font, secondary);
    //     render_centered_text(d, bottom, bottom_text, font, Color::WHITE);
    // }
}

fn render_cursor(d: &mut impl RaylibDraw, node_loc: NodeCoord, node: &ExecNode) {
    let (line, column) = node.cursor_line_column();

    let x_offset = column as f32 * consts::NODE_CHAR_WIDTH;

    let cursor_top = node_loc.line_pos(line) + Vector2::new(x_offset, 0.);
    let cursor_bottom = cursor_top + Vector2::new(0., consts::NODE_LINE_HEIGHT);

    d.draw_line_ex(
        cursor_top,
        cursor_bottom,
        consts::LINE_THICKNESS,
        Color::WHITE,
    );
}

// fn render_error_squiggle(
//     d: &mut impl RaylibDraw,
//     node_loc: NodeCoord,
//     node_text: &NodeText,
//     line_no: u8,
// ) {
//     let Some(line_len) = node_text.lines().nth(line_no as usize).map(str::len) else {
//         return;
//     };

//     let squiggle_start =
//         node_loc.line_pos(line_no as usize) + Vector2::new(0.0, consts::NODE_LINE_HEIGHT);
//     let squiggle_end =
//         squiggle_start + Vector2::new(line_len as f32 * consts::NODE_CHAR_WIDTH, 0.0);

//     d.draw_line_ex(
//         squiggle_start,
//         squiggle_end,
//         consts::LINE_THICKNESS,
//         Color::RED,
//     );
// }

fn render_io_arrow(
    d: &mut impl RaylibDraw,
    node_loc: &NodeCoord,
    dir: Dir,
    label: &str,
    font: &Font,
) {
    let indicator_center = node_loc.io_indicator(dir);

    let component_offset = dir
        .rotate_right()
        .normalized()
        .scale_by(1. / 3. * consts::NODE_OUTSIDE_PADDING);

    let arrow_center = indicator_center - component_offset;
    let text_center = indicator_center + component_offset;

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
    let dash_len = consts::NODE_OUTSIDE_SIDE_LENGTH / (2 * consts::GHOST_NODE_DASHES + 1) as f32;

    let dash_tail = (end_pos - start_pos).normalized().scale_by(dash_len);

    for dash_no in 0..=dashes {
        let dash_start = start_pos + dash_tail.scale_by(2.0 * dash_no as f32);
        d.draw_line_ex(
            dash_start,
            dash_start + dash_tail,
            consts::LINE_THICKNESS,
            color,
        );
    }
}

fn render_plus(d: &mut impl RaylibDraw, center: Vector2, color: Color) {
    d.draw_line_ex(
        center + Vector2::new(-consts::NODE_LINE_HEIGHT, 0.0),
        center + Vector2::new(consts::NODE_LINE_HEIGHT, 0.0),
        consts::LINE_THICKNESS,
        color,
    );
    d.draw_line_ex(
        center + Vector2::new(0.0, -consts::NODE_LINE_HEIGHT),
        center + Vector2::new(0.0, consts::NODE_LINE_HEIGHT),
        consts::LINE_THICKNESS,
        color,
    );
}

fn render_arrow(d: &mut impl RaylibDraw, center: Vector2, direction: Dir, color: Color) {
    let dir_vec = direction.normalized();

    let arrow_tip = center + dir_vec.scale_by(consts::NODE_LINE_HEIGHT);
    let arrow_base = center - dir_vec.scale_by(consts::NODE_LINE_HEIGHT);

    let arrow_left_wing = center
        + dir_vec
            .scale_by(consts::NODE_LINE_HEIGHT)
            .rotated((1.0 / 4.0) * f32::consts::TAU);

    let arrow_right_wing = center
        + dir_vec
            .scale_by(consts::NODE_LINE_HEIGHT)
            .rotated(-(1.0 / 4.0) * f32::consts::TAU);

    d.draw_line_ex(arrow_base, arrow_tip, consts::LINE_THICKNESS, color);
    d.draw_line_ex(arrow_tip, arrow_left_wing, consts::LINE_THICKNESS, color);
    d.draw_line_ex(arrow_tip, arrow_right_wing, consts::LINE_THICKNESS, color);
}

fn render_double_arrow(d: &mut impl RaylibDraw, center: Vector2, direction: Dir, color: Color) {
    let dir_vec = direction.normalized();

    let half_arrow_stem = dir_vec.scale_by(consts::NODE_LINE_HEIGHT);

    let arrow_tip = center + half_arrow_stem;
    let arrow_base = center - half_arrow_stem;

    let arrow_left_wing = center
        + dir_vec
            .scale_by(consts::NODE_LINE_HEIGHT)
            .rotated((1.0 / 4.0) * f32::consts::TAU);

    let arrow_right_wing = center
        + dir_vec
            .scale_by(consts::NODE_LINE_HEIGHT)
            .rotated(-(1.0 / 4.0) * f32::consts::TAU);

    d.draw_line_ex(arrow_base, arrow_tip, consts::LINE_THICKNESS, color);
    d.draw_line_ex(arrow_tip, arrow_left_wing, consts::LINE_THICKNESS, color);
    d.draw_line_ex(arrow_tip, arrow_right_wing, consts::LINE_THICKNESS, color);

    d.draw_line_ex(
        arrow_tip,
        arrow_left_wing + half_arrow_stem,
        consts::LINE_THICKNESS,
        color,
    );
    d.draw_line_ex(
        arrow_tip,
        arrow_right_wing + half_arrow_stem,
        consts::LINE_THICKNESS,
        color,
    );
}

fn render_node_border(d: &mut impl RaylibDraw, node_loc: NodeCoord, line_color: Color) {
    d.draw_line_ex(
        node_loc.top_left_corner(),
        node_loc.top_right_corner(),
        consts::LINE_THICKNESS,
        line_color,
    );
    d.draw_line_ex(
        node_loc.top_left_corner(),
        node_loc.bottom_left_corner(),
        consts::LINE_THICKNESS,
        line_color,
    );
    d.draw_line_ex(
        node_loc.bottom_left_corner(),
        node_loc.bottom_right_corner(),
        consts::LINE_THICKNESS,
        line_color,
    );
    d.draw_line_ex(
        node_loc.top_right_corner(),
        node_loc.bottom_right_corner(),
        consts::LINE_THICKNESS,
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
    let text_size = font.measure_text(text, consts::NODE_FONT_SIZE, consts::NODE_FONT_SPACING);

    let top_left = center - text_size.scale_by(0.5);

    d.draw_text_ex(
        font,
        text,
        top_left,
        consts::NODE_FONT_SIZE,
        consts::NODE_FONT_SPACING,
        color,
    );
}

fn render_dashed_node_border(d: &mut impl RaylibDraw, node_loc: NodeCoord, line_color: Color) {
    render_dashed_line(
        d,
        node_loc.top_left_corner(),
        node_loc.top_right_corner(),
        line_color,
        consts::GHOST_NODE_DASHES,
    );
    render_dashed_line(
        d,
        node_loc.top_left_corner(),
        node_loc.bottom_left_corner(),
        line_color,
        consts::GHOST_NODE_DASHES,
    );
    render_dashed_line(
        d,
        node_loc.bottom_left_corner(),
        node_loc.bottom_right_corner(),
        line_color,
        consts::GHOST_NODE_DASHES,
    );
    render_dashed_line(
        d,
        node_loc.top_right_corner(),
        node_loc.bottom_right_corner(),
        line_color,
        consts::GHOST_NODE_DASHES,
    );
}

#[derive(Clone, Debug)]
struct Input {
    mods: Modifiers,
    pressed: Option<Key>,
    window_dimensions: (i32, i32),
    mouse_wheel_move: f32,
    clipboard: String,
}

#[derive(PartialEq, Eq, Clone, Copy, Debug)]
enum Key {
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

impl Key {
    fn from(raylib_key: KeyboardKey, shift_held: bool) -> Option<Self> {
        let unbound = None;
        let handled_elsewhere = None;

        let case = if shift_held { 1 } else { 0 };

        use KeyboardKey as RK;

        Some(
            match raylib_key {
                // symbols
                RK::KEY_NULL => return None,
                RK::KEY_APOSTROPHE => [Key::Char('\''), Key::Char('"')],
                RK::KEY_COMMA => [Key::Char(','), Key::Char('<')],
                RK::KEY_MINUS => [Key::Char('-'), Key::Char('_')],
                RK::KEY_PERIOD => [Key::Char('.'), Key::Char('>')],
                RK::KEY_SLASH => [Key::Char('/'), Key::Char('?')],
                RK::KEY_SEMICOLON => [Key::Char(';'), Key::Char(':')],
                RK::KEY_EQUAL => [Key::Char('='), Key::Char('+')],
                // number keys
                RK::KEY_ZERO => [Key::Char('0'), Key::Char(')')],
                RK::KEY_ONE => [Key::Char('1'), Key::Char('!')],
                RK::KEY_TWO => [Key::Char('2'), Key::Char('@')],
                RK::KEY_THREE => [Key::Char('3'), Key::Char('#')],
                RK::KEY_FOUR => [Key::Char('4'), Key::Char('$')],
                RK::KEY_FIVE => [Key::Char('5'), Key::Char('%')],
                RK::KEY_SIX => [Key::Char('6'), Key::Char('^')],
                RK::KEY_SEVEN => [Key::Char('7'), Key::Char('&')],
                RK::KEY_EIGHT => [Key::Char('8'), Key::Char('*')],
                RK::KEY_NINE => [Key::Char('9'), Key::Char('(')],
                // alphabetical keys
                RK::KEY_A => [Key::Char('A'), Key::Char('A')],
                RK::KEY_B => [Key::Char('B'), Key::Char('B')],
                RK::KEY_C => [Key::Char('C'), Key::Char('C')],
                RK::KEY_D => [Key::Char('D'), Key::Char('D')],
                RK::KEY_E => [Key::Char('E'), Key::Char('E')],
                RK::KEY_F => [Key::Char('F'), Key::Char('F')],
                RK::KEY_G => [Key::Char('G'), Key::Char('G')],
                RK::KEY_H => [Key::Char('H'), Key::Char('H')],
                RK::KEY_I => [Key::Char('I'), Key::Char('I')],
                RK::KEY_J => [Key::Char('J'), Key::Char('J')],
                RK::KEY_K => [Key::Char('K'), Key::Char('K')],
                RK::KEY_L => [Key::Char('L'), Key::Char('L')],
                RK::KEY_M => [Key::Char('M'), Key::Char('M')],
                RK::KEY_N => [Key::Char('N'), Key::Char('N')],
                RK::KEY_O => [Key::Char('O'), Key::Char('O')],
                RK::KEY_P => [Key::Char('P'), Key::Char('P')],
                RK::KEY_Q => [Key::Char('Q'), Key::Char('Q')],
                RK::KEY_R => [Key::Char('R'), Key::Char('R')],
                RK::KEY_S => [Key::Char('S'), Key::Char('S')],
                RK::KEY_T => [Key::Char('T'), Key::Char('T')],
                RK::KEY_U => [Key::Char('U'), Key::Char('U')],
                RK::KEY_V => [Key::Char('V'), Key::Char('V')],
                RK::KEY_W => [Key::Char('W'), Key::Char('W')],
                RK::KEY_X => [Key::Char('X'), Key::Char('X')],
                RK::KEY_Y => [Key::Char('Y'), Key::Char('Y')],
                RK::KEY_Z => [Key::Char('Z'), Key::Char('Z')],
                RK::KEY_LEFT_BRACKET => [Key::Char('['), Key::Char('{')],
                RK::KEY_BACKSLASH => [Key::Char('\\'), Key::Char('|')],
                RK::KEY_RIGHT_BRACKET => [Key::Char(']'), Key::Char('}')],
                RK::KEY_GRAVE => [Key::Char('`'), Key::Char('~')],
                RK::KEY_SPACE => [Key::Char(' '), Key::Char(' ')],
                RK::KEY_ESCAPE => [Key::Esc, Key::Esc],
                // nav/special edit keys
                RK::KEY_ENTER => [Key::Enter, Key::Enter],
                RK::KEY_TAB => [Key::Tab, Key::Tab],
                RK::KEY_BACKSPACE => [Key::Backspace, Key::Backspace],
                RK::KEY_INSERT => return unbound,
                RK::KEY_DELETE => [Key::Delete, Key::Delete],
                RK::KEY_RIGHT => [Key::Arrow(Dir::Right), Key::Arrow(Dir::Right)],
                RK::KEY_LEFT => [Key::Arrow(Dir::Left), Key::Arrow(Dir::Left)],
                RK::KEY_DOWN => [Key::Arrow(Dir::Down), Key::Arrow(Dir::Down)],
                RK::KEY_UP => [Key::Arrow(Dir::Up), Key::Arrow(Dir::Up)],
                RK::KEY_PAGE_UP => return unbound,
                RK::KEY_PAGE_DOWN => return unbound,
                RK::KEY_HOME => [Key::Home, Key::Home],
                RK::KEY_END => [Key::End, Key::End],
                RK::KEY_CAPS_LOCK => return unbound,
                RK::KEY_SCROLL_LOCK => return unbound,
                RK::KEY_NUM_LOCK => return unbound,
                RK::KEY_PRINT_SCREEN => return unbound,
                RK::KEY_PAUSE => return unbound,
                // f-keys
                RK::KEY_F1 => return unbound,
                RK::KEY_F2 => return unbound,
                RK::KEY_F3 => return unbound,
                RK::KEY_F4 => return unbound,
                RK::KEY_F5 => return unbound,
                RK::KEY_F6 => return unbound,
                RK::KEY_F7 => return unbound,
                RK::KEY_F8 => return unbound,
                RK::KEY_F9 => return unbound,
                RK::KEY_F10 => return unbound,
                RK::KEY_F11 => return unbound,
                RK::KEY_F12 => return unbound,
                // modifiers
                RK::KEY_LEFT_SHIFT => return handled_elsewhere,
                RK::KEY_LEFT_CONTROL => return handled_elsewhere,
                RK::KEY_LEFT_ALT => return unbound,
                RK::KEY_LEFT_SUPER => return unbound,
                RK::KEY_RIGHT_SHIFT => return handled_elsewhere,
                RK::KEY_RIGHT_CONTROL => return handled_elsewhere,
                RK::KEY_RIGHT_ALT => return unbound,
                RK::KEY_RIGHT_SUPER => return unbound,
                RK::KEY_KB_MENU => return unbound,
                // keypad
                RK::KEY_KP_0 => return unbound,
                RK::KEY_KP_1 => [Key::End, Key::End],
                RK::KEY_KP_2 => return unbound,
                RK::KEY_KP_3 => return unbound,
                RK::KEY_KP_4 => return unbound,
                RK::KEY_KP_5 => return unbound,
                RK::KEY_KP_6 => return unbound,
                RK::KEY_KP_7 => [Key::Home, Key::Home],
                RK::KEY_KP_8 => return unbound,
                RK::KEY_KP_9 => return unbound,
                RK::KEY_KP_DECIMAL => return unbound,
                RK::KEY_KP_DIVIDE => return unbound,
                RK::KEY_KP_MULTIPLY => return unbound,
                RK::KEY_KP_SUBTRACT => return unbound,
                RK::KEY_KP_ADD => return unbound,
                RK::KEY_KP_ENTER => return unbound,
                RK::KEY_KP_EQUAL => return unbound,
                // unknown
                RK::KEY_BACK => todo!("what key is this?"),
                // volume
                RK::KEY_VOLUME_UP => return unbound,
                RK::KEY_VOLUME_DOWN => return unbound,
            }[case],
        )
    }
}

enum RepeatKey {
    None,
    Held { key: KeyboardKey, repeat_delay: f32 },
}

fn get_input(rl: &mut RaylibHandle, repeat: &mut RepeatKey) -> Input {
    let ctrl_held = rl.is_key_down(KeyboardKey::KEY_LEFT_CONTROL)
        || rl.is_key_down(KeyboardKey::KEY_RIGHT_CONTROL);

    let shift_held =
        rl.is_key_down(KeyboardKey::KEY_LEFT_SHIFT) || rl.is_key_down(KeyboardKey::KEY_RIGHT_SHIFT);

    let mods = match (ctrl_held, shift_held) {
        (true, true) => Modifiers::CtrlShift,
        (true, false) => Modifiers::Ctrl,
        (false, true) => Modifiers::Shift,
        (false, false) => Modifiers::None,
    };

    let raylib_key_pressed = rl.get_key_pressed();

    let pressed = if let Some(key) = raylib_key_pressed {
        *repeat = RepeatKey::Held {
            key,
            repeat_delay: consts::KEY_REPEAT_DELAY_S,
        };
        raylib_key_pressed.and_then(|rk| Key::from(rk, shift_held))
    } else if let RepeatKey::Held { key, repeat_delay } = repeat
        && rl.is_key_down(*key)
    {
        *repeat_delay -= rl.get_frame_time();
        if *repeat_delay <= 0.0 {
            *repeat_delay = consts::KEY_REPEAT_INTERVAL_S;
            Key::from(*key, shift_held)
        } else {
            None
        }
    } else {
        *repeat = RepeatKey::None;
        raylib_key_pressed.and_then(|rk| Key::from(rk, shift_held))
    };

    let clipboard = match rl.get_clipboard_text() {
        Ok(text) if text.is_ascii() => text.to_ascii_uppercase(),

        Ok(_) | Err(_) => String::new(),
    };

    Input {
        mods,
        pressed,
        window_dimensions: (rl.get_screen_width(), rl.get_screen_height()),
        mouse_wheel_move: rl.get_mouse_wheel_move(),
        clipboard,
    }
}

struct Output {
    clipboard: Option<String>,
}

enum Update<T> {
    Exit,
    Update { new: T, output: Output },
}

impl<T> Update<T> {
    fn no_output(new: T) -> Self {
        Update::Update {
            new,
            output: Output { clipboard: None },
        }
    }
}

fn update(state: State, input: Input) -> Update<State> {
    match handle_input(state.model, &input) {
        Update::Exit => {
            return Update::Exit;
        }

        Update::Update { new, output } => {
            let camera = update_camera(
                state.camera,
                new.highlighted_node,
                input.window_dimensions,
                input.mouse_wheel_move,
            );

            Update::Update {
                new: State { camera, model: new },
                output,
            }
        }
    }
}

fn handle_input(model: Model, input: &Input) -> Update<Model> {
    // the old ghosts value should not be reused, this enforces it
    std::mem::drop(model.ghosts);

    let ghosts = match input.mods {
        Modifiers::Ctrl => Ghosts::MoveView,
        Modifiers::CtrlShift => Ghosts::MoveNode,
        Modifiers::Shift | Modifiers::None => Ghosts::None,
    };

    let Some(pressed) = input.pressed else {
        return Update::no_output(Model { ghosts, ..model });
    };

    match (input.mods, pressed) {
        (_, Key::Esc) => {
            let mut nodes = model.nodes;

            let stop_result = stop_execution(&mut model.nodes, model.highlighted_node);

            match stop_result {
                StopResult::Stopped => Update::no_output(Model {
                    ghosts,
                    nodes,
                    ..model
                }),

                StopResult::WasAlreadyStopped => Update::Exit,
            }
        }

        (Modifiers::None, Key::Tab) => {
            if let Some(updated_nodes) = step_execution(&model.nodes, model.highlighted_node) {
                let mut nodes = model.nodes;

                nodes.extend(updated_nodes);

                Update::no_output(Model {
                    nodes,
                    ghosts,
                    ..model
                })
            } else {
                Update::no_output(Model { ghosts, ..model })
            }
        }

        (mods @ (Modifiers::None | Modifiers::Shift), Key::Arrow(dir)) => {
            let mut nodes = model.nodes;
            match &mut nodes.get_mut(&model.highlighted_node).map(|n| n.variant) {
                Some(NodeType::Exec(exec_node)) => {
                    let select = mods == Modifiers::Shift;

                    match dir {
                        Dir::Up => exec_node.up(select),
                        Dir::Down => exec_node.down(select),
                        Dir::Left => exec_node.left(select),
                        Dir::Right => exec_node.right(select),
                    };

                    if !select {
                        exec_node.deselect();
                    }

                    Update::no_output(Model {
                        nodes,
                        ghosts,
                        ..model
                    })
                }

                None | Some(NodeType::Input(_)) => Update::no_output(Model {
                    nodes,
                    ghosts,
                    ..model
                }),
            }
        }

        (Modifiers::Ctrl, Key::Arrow(dir)) => Update::no_output(Model {
            highlighted_node: model.highlighted_node.neighbor(dir),
            ghosts,
            ..model
        }),

        (Modifiers::CtrlShift, Key::Arrow(dir)) => {
            let mut nodes = model.nodes;
            let src = model.highlighted_node;
            let dst = model.highlighted_node.neighbor(dir);
            if nodes.contains_key(&src) && !nodes.contains_key(&dst) {
                let node = nodes.remove(&src).unwrap();

                nodes.try_insert(dst, node).unwrap();

                Update::no_output(Model {
                    nodes,
                    ghosts,
                    highlighted_node: dst,
                    ..model
                })
            } else {
                Update::no_output(Model {
                    nodes,
                    ghosts,
                    ..model
                })
            }
        }

        (Modifiers::None, Key::Delete) => {
            let mut nodes = model.nodes;

            nodes.remove(&model.highlighted_node);

            Update::no_output(Model {
                nodes,
                ghosts,
                ..model
            })
        }

        (Modifiers::Ctrl, Key::Char('A')) => {
            let mut nodes = model.nodes;

            if let Some(NodeType::Exec(exec_node)) =
                nodes.get_mut(&model.highlighted_node).map(|n| n.variant)
            {
                exec_node.select_all();

                Update::no_output(Model {
                    nodes,
                    ghosts,
                    ..model
                })
            } else {
                Update::no_output(Model {
                    nodes,
                    ghosts,
                    ..model
                })
            }
        }

        (Modifiers::Ctrl, Key::Char('C')) => {
            if let Some(node) = model.nodes.get(&model.highlighted_node) {
                match &node.variant {
                    NodeType::Exec(exec_node) if exec_node.text_selected() => {
                        let selection = exec_node.selection().to_string();
                        Update::Update {
                            new: Model {
                                ghosts,
                                node_clipboard: None,
                                ..model
                            },
                            output: Output {
                                clipboard: Some(selection),
                            },
                        }
                    }

                    NodeType::Exec(exec_node) => {
                        let node_text = exec_node.text().to_string();

                        Update::Update {
                            new: Model {
                                ghosts,
                                node_clipboard: Some(node.clone()),
                                ..model
                            },
                            output: Output {
                                clipboard: Some(node_text),
                            },
                        }
                    }

                    // TODO: maybe this should copy the input data to
                    // the system clipboard too?
                    NodeType::Input(_input_node) => Update::no_output(Model {
                        ghosts,
                        node_clipboard: Some(node.clone()),
                        ..model
                    }),
                }
            } else {
                Update::no_output(Model { ghosts, ..model })
            }
        }

        (Modifiers::Ctrl, Key::Char('X')) => {
            let mut nodes = model.nodes;

            match nodes.entry(model.highlighted_node) {
                Entry::Vacant(_) => Update::no_output(Model {
                    ghosts,
                    nodes,
                    ..model
                }),

                Entry::Occupied(mut entry) => match &mut entry.get_mut().variant {
                    NodeType::Exec(exec_node) if exec_node.text_selected() => {
                        let selection = exec_node.selection().to_string();

                        exec_node.insert("");

                        Update::Update {
                            new: Model {
                                ghosts,
                                nodes,
                                ..model
                            },
                            output: Output {
                                clipboard: Some(selection),
                            },
                        }
                    }

                    NodeType::Exec(_) | NodeType::Input(_) => {
                        let cut_node = entry.remove();

                        Update::no_output(Model {
                            ghosts,
                            nodes,
                            node_clipboard: Some(cut_node),
                            ..model
                        })
                    }
                },
            }
        }

        (Modifiers::Ctrl, Key::Char('V')) => {
            let mut nodes = model.nodes;

            match (&model.node_clipboard, nodes.entry(model.highlighted_node)) {
                (Some(copied_node), Entry::Vacant(vacant_entry)) => {
                    vacant_entry.insert(copied_node.clone());

                    Update::no_output(Model {
                        nodes,
                        ghosts,
                        ..model
                    })
                }

                (_, Entry::Occupied(mut occupied_entry)) => {
                    match &mut occupied_entry.get_mut().variant {
                        NodeType::Exec(exec_node) => {
                            exec_node.insert(&input.clipboard);

                            Update::no_output(Model {
                                ghosts,
                                nodes,
                                ..model
                            })
                        }

                        NodeType::Input(_) => Update::no_output(Model {
                            ghosts,
                            nodes,
                            ..model
                        }),
                    }
                }

                (None, Entry::Vacant(_)) => Update::no_output(Model {
                    ghosts,
                    nodes,
                    ..model
                }),
            }
        }

        (Modifiers::Ctrl, Key::Char('O')) => {
            if let Some(path) = rfd::FileDialog::new()
                .set_title("Load TIS workspace from file")
                .add_filter("TIS workspace", &["toml"])
                .pick_file()
            {
                match std::fs::read_to_string(path) {
                    Ok(toml) => match parse_toml(&toml) {
                        Ok((nodes, highlighted_node)) => Update::no_output(Model {
                            nodes,
                            highlighted_node,
                            ghosts,
                            ..model
                        }),

                        Err(import_err) => {
                            let origin = NodeCoord::at(0, 0);
                            let description = match import_err {
                                ImportErr::InvalidToml => "# INVALID TOML",
                                ImportErr::InvalidCoord => "# INVALID COORD",
                                ImportErr::NodeTextDoesntFit => "# CODE DOESN'T FIT",
                                ImportErr::InvalidRhs => "# INVALID RHS",
                                ImportErr::DuplicateCoord => "# DUPLICATE COORD",
                                ImportErr::InvalidHighlightRhs => "# INVALID LOC",
                                ImportErr::IntOutOfRange => "# INT OVERFLOW",
                                ImportErr::NotAnInt => "# NOT AN INT",
                            };

                            let node =
                                Node::exec_with_lines(["## ERROR", "", description]).unwrap();

                            let nodes = Nodes::from([(origin, node)]);

                            Update::no_output(Model {
                                nodes,
                                ghosts,
                                ..model
                            })
                        }
                    },

                    Err(_) => {
                        let origin = NodeCoord::at(0, 0);

                        let node = Node::exec_with_lines([
                            "## ERROR",
                            "",
                            "# COULD NOT OPEN",
                            "# SPECIFIED FILE",
                        ])
                        .unwrap();

                        let nodes = Nodes::from([(origin, node)]);

                        Update::no_output(Model {
                            nodes,
                            ghosts,
                            ..model
                        })
                    }
                }
            } else {
                Update::no_output(Model { ghosts, ..model })
            }
        }

        (Modifiers::Ctrl, Key::Char('S')) => {
            if let Some(path) = rfd::FileDialog::new()
                .set_title("Save TIS workspace to file")
                .add_filter("TIS workspace", &["toml"])
                .set_file_name("my_tis_workspace.toml")
                .save_file()
            {
                let toml = serialize_toml(&model.nodes, Some(model.highlighted_node));

                match std::fs::write(path, toml) {
                    Ok(()) => Update::no_output(Model { ghosts, ..model }),

                    Err(err) => {
                        // TODO: show this to the user
                        println!("io error while saving file: {:?}", err);
                        Update::no_output(Model { ghosts, ..model })
                    }
                }
            } else {
                Update::no_output(Model { ghosts, ..model })
            }
        }

        (Modifiers::None | Modifiers::Shift, Key::Char(char)) => {
            let mut nodes = model.nodes;

            match nodes.entry(model.highlighted_node) {
                Entry::Occupied(mut occupied) => {
                    match &mut occupied.get_mut().variant {
                        NodeType::Exec(exec_node) => {
                            // apparently this is the easiest way to turn a `char` into a `&str`
                            // (without allocating a single-char `String` first`)
                            let mut buf = [0; std::mem::size_of::<char>()];

                            exec_node.insert(char.encode_utf8(&mut buf));
                        }

                        NodeType::Input(_) => {
                            // TODO: handle direct node input?
                        }
                    }
                }

                Entry::Vacant(vacant) => match char {
                    'E' => {
                        vacant.insert(Node::empty_exec());
                    }
                    'I' => {
                        vacant.insert(Node::empty_input());
                    }
                    _ => {}
                },
            }

            Update::no_output(Model {
                nodes,
                ghosts,
                ..model
            })
        }

        (mods @ (Modifiers::None | Modifiers::Shift), Key::Home) => {
            let mut nodes = model.nodes;

            if let Some(NodeType::Exec(exec_node)) =
                &mut nodes.get_mut(&model.highlighted_node).map(|n| n.variant)
            {
                let select = mods == Modifiers::Shift;

                exec_node.home(select);

                if !select {
                    exec_node.deselect();
                }
            }

            Update::no_output(Model {
                nodes,
                ghosts,
                ..model
            })
        }

        (mods @ (Modifiers::None | Modifiers::Shift), Key::End) => {
            let mut nodes = model.nodes;

            if let Some(NodeType::Exec(exec_node)) =
                &mut nodes.get_mut(&model.highlighted_node).map(|n| n.variant)
            {
                let select = mods == Modifiers::Shift;

                exec_node.end(select);

                if !select {
                    exec_node.deselect();
                }
            }

            Update::no_output(Model {
                nodes,
                ghosts,
                ..model
            })
        }

        (Modifiers::None, Key::Backspace) => {
            let mut nodes = model.nodes;

            if let Some(NodeType::Exec(exec_node)) =
                &mut nodes.get_mut(&model.highlighted_node).map(|n| n.variant)
            {
                exec_node.backspace();
            }

            Update::no_output(Model {
                nodes,
                ghosts,
                ..model
            })
        }

        (mods @ (Modifiers::None | Modifiers::Shift), Key::Enter) => {
            let mut nodes = model.nodes;

            if let Some(NodeType::Exec(exec_node)) =
                &mut nodes.get_mut(&model.highlighted_node).map(|n| n.variant)
            {
                let select = mods == Modifiers::Shift;

                exec_node.enter(select);
            }

            Update::no_output(Model {
                nodes,
                ghosts,
                ..model
            })
        }

        (
            Modifiers::Ctrl | Modifiers::CtrlShift,
            Key::Backspace
            | Key::Delete
            | Key::Enter
            | Key::Home
            | Key::End
            | Key::Tab
            | Key::Char(_),
        )
        | (Modifiers::Shift, Key::Backspace | Key::Delete | Key::Tab) => {
            Update::no_output(Model { ghosts, ..model })
        }
    }
}

fn seek_nodes(nodes: &Nodes, start: NodeCoord) -> SortedSet<NodeCoord> {
    fn helper(nodes: &Nodes, current: NodeCoord, set: &mut SortedSet<NodeCoord>) {
        if !set.contains(&current) {
            for dir in Dir::ALL {
                helper(nodes, current.neighbor(dir), set);
            }
        }
    }

    let mut set = SortedSet::new();

    set.insert(start);

    helper(nodes, start, &mut set);

    set
}

enum StopResult {
    WasAlreadyStopped,
    Stopped,
}

impl StopResult {
    fn reconcile(&mut self, other: Self) {
        match self {
            Self::WasAlreadyStopped => *self = other,
            Self::Stopped => {}
        }
    }
}

fn stop_execution(nodes: &mut Nodes, start: NodeCoord) -> StopResult {
    let mut stop_result = StopResult::WasAlreadyStopped;

    for node_loc in seek_nodes(&nodes, start) {
        // TODO: can this expect be removed somehow?
        let node = nodes
            .get_mut(&node_loc)
            .expect("seek_nodes() shouldn't return a NodeLoc that isn't occupied");

        stop_result.reconcile(stop_node_execution(node))
    }

    stop_result
}

fn stop_node_execution(nodes: &mut Node) -> StopResult {
    todo!()
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

#[derive(Clone, Debug)]
struct NodeExec {
    acc: Num,
    bak: Num,
    ip: u8,
}

#[derive(Debug)]
enum ImportErr {
    InvalidToml,
    InvalidCoord,
    NodeTextDoesntFit,
    InvalidRhs,
    DuplicateCoord,
    InvalidHighlightRhs,
    IntOutOfRange,
    NotAnInt,
}

use toml::{Table, Value};

const HIGHLIGHTED_NODE_KEY: &'static str = "highlighted";

fn parse_toml(toml: &str) -> Result<(Nodes, NodeCoord), ImportErr> {
    let table: Table = match toml::from_str(toml) {
        Ok(table) => table,
        Err(_) => return Err(ImportErr::InvalidToml),
    };

    let mut nodes = Nodes::new();
    let mut highlighted = None;

    for (key, value) in table {
        if &key == HIGHLIGHTED_NODE_KEY {
            if let Value::String(coord) = value {
                highlighted = Some(parse_coord(&coord)?);
            } else {
                return Err(ImportErr::InvalidHighlightRhs);
            }
        } else {
            let (node_loc, node) = parse_node(&key, value)?;

            if nodes.try_insert(node_loc, node).is_err() {
                return Err(ImportErr::DuplicateCoord);
            };
        }
    }

    Ok((nodes, highlighted.unwrap_or(NodeCoord::at(0, 0))))
}

fn parse_node(key: &str, value: Value) -> Result<(NodeCoord, Node), ImportErr> {
    let node_loc = parse_coord(key)?;

    let node = match value {
        Value::String(text) => {
            Node::exec_with_text(text.trim_end()).ok_or(ImportErr::NodeTextDoesntFit)?
        }

        Value::Array(arr) => {
            let data = arr
                .into_iter()
                .map(|value| {
                    if let Value::Integer(int) = value {
                        int.try_into().map_err(|_| ImportErr::IntOutOfRange)
                    } else {
                        Err(ImportErr::NotAnInt)
                    }
                })
                .try_collect()?;

            Node::input_with_data(data)
        }

        _ => return Err(ImportErr::InvalidRhs),
    };

    Ok((node_loc, node))
}

fn parse_coord(str: &str) -> Result<NodeCoord, ImportErr> {
    let mut coords = str.split(',');

    let x = coords
        .next()
        .ok_or(ImportErr::InvalidCoord)?
        .trim()
        .parse()
        .map_err(|_| ImportErr::InvalidCoord)?;

    let y = coords
        .next()
        .ok_or(ImportErr::InvalidCoord)?
        .trim()
        .parse()
        .map_err(|_| ImportErr::InvalidCoord)?;

    Ok(NodeCoord::at(x, y))
}

fn fmt_coord(node_loc: &NodeCoord) -> String {
    format!("{}, {}", node_loc.x, node_loc.y)
}

fn serialize_toml(nodes: &Nodes, highlighted_node: Option<NodeCoord>) -> String {
    todo!()
    // let mut toml = String::new();

    // for (node_loc, node) in nodes {
    //     let key = fmt_coord(node_loc);

    //     toml += &match node {
    //         NodeType::Exec(exec_node) => {
    //             format!("\"{}\" = \"\"\"\n{}\n\"\"\"\n\n", key, &exec_node.text)
    //         }
    //         NodeType::Input(input_node) => {
    //             let mut fmt = format!("\"{}\" = [ ", key);

    //             for num in &input_node.data {
    //                 fmt += &format!("{num}, ");
    //             }

    //             fmt + "]\n\n"
    //         }
    //     };
    // }

    // if let Some(highlighted) = highlighted_node {
    //     toml += &format!("{HIGHLIGHTED_NODE_KEY} = \"{}\"", fmt_coord(&highlighted));
    // }

    // toml
}
