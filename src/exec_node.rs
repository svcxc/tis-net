use crate::NodeOutbox;
use crate::consts;
use crate::node::Dir;
use crate::node::Num;
use crate::node::StopResult;
use arrayvec::{ArrayString, ArrayVec};

#[derive(Clone, Debug)]
pub struct ExecNode {
    text: NodeText,
    cursor: usize,
    select_cursor: usize,
    state: ExecNodeState,
}

#[derive(Clone, Debug)]
pub enum ExecNodeState {
    Empty,
    Errored(ParseErr),
    Ready(NodeCode),
    Running(ExecNodeRuntime),
}

#[derive(Clone, Debug)]
pub struct ExecNodeRuntime {
    code: NodeCode,
    ip: u8,
    acc: Num,
    bak: Num,
    last: Option<Dir>,
}

impl ExecNodeRuntime {
    fn inc_ip(&mut self) {
        self.ip += 1;
        if self.ip as usize >= self.code.len() {
            self.ip = 0;
        }
    }

    fn read_continuation<'node>(
        &'node mut self,
        and_then: ReadContinuation,
    ) -> impl FnMut(Num) -> NodeOutbox + use<'node> {
        move |input| match and_then {
            ReadContinuation::Mov(ResolvedDst::Nil) => {
                self.inc_ip();
                NodeOutbox::Empty
            }
            ReadContinuation::Mov(ResolvedDst::Acc) => {
                self.acc = input;
                self.inc_ip();
                NodeOutbox::Empty
            }
            ReadContinuation::Mov(ResolvedDst::Dir(dir)) => NodeOutbox::Directional(dir, input),
            ReadContinuation::Mov(ResolvedDst::Any) => NodeOutbox::Any(input),
            ReadContinuation::Add => {
                self.acc = self.acc.saturating_add(input);
                self.inc_ip();
                NodeOutbox::Empty
            }
            ReadContinuation::Sub => {
                self.acc = self.acc.saturating_sub(input);
                self.inc_ip();
                NodeOutbox::Empty
            }
            ReadContinuation::Jro => {
                if input < 0 {
                    self.ip = self.ip.saturating_sub(input.abs() as u8);
                } else {
                    self.ip = self.ip.saturating_add(input as u8);
                    if self.ip as usize >= self.code.len() {
                        self.ip = (self.code.len() - 1) as u8;
                    }
                }
                NodeOutbox::Empty
            }
        }
    }

    fn resolve_src(&self, src: Src) -> ResolvedSrc {
        match src {
            Src::Acc => ResolvedSrc::Value(self.acc),
            Src::Imm(immediate) => ResolvedSrc::Value(immediate),
            Src::Nil => ResolvedSrc::Value(0),
            Src::Dir(dir) => ResolvedSrc::Dir(dir),
            Src::Any => ResolvedSrc::Any,
            Src::Last => {
                if let Some(last) = self.last {
                    ResolvedSrc::Dir(last)
                } else {
                    ResolvedSrc::Value(0)
                }
            }
        }
    }

    fn resolve_dst(&self, dst: Dst) -> ResolvedDst {
        match dst {
            Dst::Acc => ResolvedDst::Acc,
            Dst::Dir(dir) => ResolvedDst::Dir(dir),
            Dst::Any => ResolvedDst::Any,
            Dst::Last => {
                if let Some(last) = self.last {
                    ResolvedDst::Dir(last)
                } else {
                    ResolvedDst::Nil
                }
            }
            Dst::Nil => ResolvedDst::Nil,
        }
    }
}

pub struct Gizmos {
    pub acc: ArrayString<4>,
    pub bak: ArrayString<4>,
    pub last: &'static str,
    pub mode: &'static str,
}

enum ResolvedSrc {
    Value(Num),
    Dir(Dir),
    Any,
}

enum ResolvedDst {
    Nil,
    Acc,
    Dir(Dir),
    Any,
}

#[derive(Clone, Debug)]
enum Mode {
    Exec,
    Read,
    Write,
}

type NodeText = ArrayString<{ consts::NODE_TEXT_BUFFER_SIZE }>;

impl ExecNode {
    pub fn empty() -> Self {
        Self {
            text: ArrayString::new(),
            cursor: 0,
            select_cursor: 0,
            state: ExecNodeState::Empty,
        }
    }

    pub fn with_text(text: &str) -> Option<Self> {
        let text = ArrayString::from(text).ok()?;

        if !validate_text_dimensions(&text) {
            return None;
        }

        let state = update_state(&text);

        Some(ExecNode {
            text,
            cursor: 0,
            select_cursor: 0,
            state,
        })
    }

    pub fn state(&self) -> &ExecNodeState {
        &self.state
    }

    pub fn text(&self) -> &str {
        &self.text
    }

    pub fn is_in_edit_mode(&self) -> bool {
        match self.state {
            ExecNodeState::Running { .. } => true,
            ExecNodeState::Empty | ExecNodeState::Errored(_) | ExecNodeState::Ready(_) => false,
        }
    }

    pub fn cursor_at_error_line(&self, error_line: u8) -> bool {
        let (select_start, select_end) = self.selection_range();
        let select_start_line = line_column(&self.text, select_start).0;
        let select_end_line = line_column(&self.text, select_end).0;

        let error_line = error_line as usize;

        select_start_line <= error_line && error_line <= select_end_line
    }

    pub fn backspace(&mut self) {
        if self.text_selected() {
            self.insert("");
        } else {
            let Some(index) = self.cursor.checked_sub(1) else {
                return;
            };

            self.text.remove(index);
            self.cursor = index;
            self.select_cursor = index;
            self.state = update_state(&self.text);
        }
    }

    pub fn text_selected(&self) -> bool {
        self.cursor != self.select_cursor
    }

    fn selection_range(&self) -> (usize, usize) {
        if self.cursor > self.select_cursor {
            (self.select_cursor, self.cursor)
        } else {
            (self.cursor, self.select_cursor)
        }
    }

    /// if text is selected, this replaces it
    pub fn insert(&mut self, txt: &str) {
        let (select_start, select_end) = self.selection_range();

        let mut new_text = ArrayString::new();

        let push_results = [
            new_text.try_push_str(&self.text[..select_start]),
            new_text.try_push_str(txt),
            new_text.try_push_str(&self.text[select_end..]),
        ];

        if push_results.iter().all(Result::is_ok) && validate_text_dimensions(&new_text) {
            self.text = new_text;
            self.cursor = select_start + txt.len();
            self.deselect();
            self.state = update_state(&self.text);
        }
    }

    pub fn selection(&self) -> &str {
        let (select_start, select_end) = self.selection_range();

        &self.text[select_start..select_end]
    }

    pub fn enter(&mut self, select: bool) {
        self.insert("\n");

        if !select {
            self.deselect();
        }
    }

    pub fn right(&mut self, select: bool) {
        self.cursor = usize::min(self.cursor + 1, self.text.len());

        if !select {
            self.deselect();
        }
    }

    pub fn left(&mut self, select: bool) {
        self.cursor = self.cursor.saturating_sub(1);

        if !select {
            self.deselect();
        }
    }

    fn target(&self, target_line: usize, target_column: usize) -> usize {
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

        cursor
    }

    pub fn up(&mut self, select: bool) {
        let (line, target_column) = line_column(&self.text, self.cursor);

        self.cursor = line
            .checked_sub(1)
            .map(|target_line| self.target(target_line, target_column))
            .unwrap_or(0);

        if !select {
            self.deselect();
        }
    }

    pub fn down(&mut self, select: bool) {
        let (line, target_column) = line_column(&self.text, self.cursor);

        let target_line = line + 1;

        self.cursor = self.target(target_line, target_column);

        if !select {
            self.deselect();
        }
    }

    pub fn home(&mut self, select: bool) {
        let mut cursor = self.cursor;

        for char in self.text.chars().rev().skip(self.text.len() - self.cursor) {
            if char == '\n' {
                break;
            } else {
                cursor -= 1;
            }
        }

        self.cursor = cursor;

        if !select {
            self.deselect();
        }
    }

    pub fn end(&mut self, select: bool) {
        let mut cursor = self.cursor;

        for char in self.text.chars().skip(self.cursor) {
            if char == '\n' {
                break;
            } else {
                cursor += 1;
            }
        }

        self.cursor = cursor;

        if !select {
            self.deselect();
        }
    }

    pub fn deselect(&mut self) {
        self.select_cursor = self.cursor;
    }

    pub fn select_all(&mut self) {
        self.select_cursor = 0;
        self.cursor = self.text.len();
    }

    pub fn cursor_line_column(&self) -> (usize, usize) {
        line_column(&self.text, self.cursor)
    }

    pub fn stop(&mut self) -> StopResult {
        let old_state = std::mem::replace(&mut self.state, ExecNodeState::Empty);

        match old_state {
            ExecNodeState::Running(ExecNodeRuntime { code, .. }) => {
                self.state = ExecNodeState::Ready(code);
                StopResult::Stopped
            }

            other
            @ (ExecNodeState::Empty | ExecNodeState::Errored(_) | ExecNodeState::Ready(_)) => {
                self.state = other;
                StopResult::WasAlreadyStopped
            }
        }
    }

    pub fn step<'node>(&'node mut self) -> ExecNodeIO<impl FnMut(Num) -> NodeOutbox + use<'node>> {
        let state = &mut self.state;

        let code = match state {
            ExecNodeState::Empty | ExecNodeState::Errored(_) => return ExecNodeIO::None,

            ExecNodeState::Ready(code) => {
                // this clone does not theoretically need to exist I think
                // we could take it out by making this function take ownership of self and then returning it again
                // that way we could partially move `code` to its new place in the ExecNodeState::Running that gets constructed below I think
                // however, that seems a lot of hassle right now compared to just doing a .clone()
                code.clone()
            }

            ExecNodeState::Running(runtime) => {
                return match runtime.code[runtime.ip as usize].op {
                    Op::Mov(src, dst) => {
                        let resolved_src = runtime.resolve_src(src);
                        let resolved_dst = runtime.resolve_dst(dst);

                        match resolved_src {
                            ResolvedSrc::Value(input) => match resolved_dst {
                                ResolvedDst::Nil => {
                                    runtime.inc_ip();
                                    ExecNodeIO::None
                                }
                                ResolvedDst::Acc => {
                                    runtime.inc_ip();
                                    runtime.acc = input;
                                    ExecNodeIO::None
                                }
                                ResolvedDst::Dir(dir) => {
                                    ExecNodeIO::Out(NodeOutbox::Directional(dir, input))
                                }
                                ResolvedDst::Any => ExecNodeIO::Out(NodeOutbox::Any(input)),
                            },
                            ResolvedSrc::Dir(dir) => ExecNodeIO::InDir(
                                dir,
                                runtime.read_continuation(ReadContinuation::Mov(resolved_dst)),
                            ),
                            ResolvedSrc::Any => ExecNodeIO::InAny(
                                runtime.read_continuation(ReadContinuation::Mov(resolved_dst)),
                            ),
                        }
                    }
                    Op::Nop => {
                        runtime.inc_ip();
                        ExecNodeIO::None
                    }
                    Op::Swp => todo!(),
                    Op::Sav => todo!(),
                    Op::Add(src) => match runtime.resolve_src(src) {
                        ResolvedSrc::Value(value) => {
                            runtime.acc.saturating_add(value);
                            runtime.inc_ip();
                            ExecNodeIO::None
                        }
                        ResolvedSrc::Dir(dir) => {
                            ExecNodeIO::InDir(dir, runtime.read_continuation(ReadContinuation::Add))
                        }
                        ResolvedSrc::Any => {
                            ExecNodeIO::InAny(runtime.read_continuation(ReadContinuation::Add))
                        }
                    },
                    Op::Sub(src) => match runtime.resolve_src(src) {
                        ResolvedSrc::Value(value) => {
                            runtime.acc.saturating_sub(value);
                            runtime.inc_ip();
                            ExecNodeIO::None
                        }
                        ResolvedSrc::Dir(dir) => {
                            ExecNodeIO::InDir(dir, runtime.read_continuation(ReadContinuation::Sub))
                        }
                        ResolvedSrc::Any => {
                            ExecNodeIO::InAny(runtime.read_continuation(ReadContinuation::Sub))
                        }
                    },
                    Op::Neg => {
                        runtime.acc = runtime.acc.saturating_neg();
                        runtime.inc_ip();
                        ExecNodeIO::None
                    }
                    Op::Jmp(_) => todo!(),
                    Op::Jez(_) => todo!(),
                    Op::Jnz(_) => todo!(),
                    Op::Jgz(_) => todo!(),
                    Op::Jlz(_) => todo!(),
                    Op::Jro(_) => todo!(),
                };
            }
        };

        *state = ExecNodeState::Running(ExecNodeRuntime {
            code,
            ip: 0,
            acc: 0,
            bak: 0,
            last: None,
        });

        ExecNodeIO::None
    }

    pub fn gizmos(&self) -> Gizmos {
        let ExecNodeState::Running(runtime) = &self.state else {
            return Gizmos {
                acc: ArrayString::from("0").unwrap(),
                bak: ArrayString::from("(0)").unwrap(),
                last: "N/A",
                mode: "EDIT",
            };
        };

        use std::fmt::Write;

        let mut acc = ArrayString::new();
        let mut bak = ArrayString::new();

        write!(&mut acc, "{}", runtime.acc);

        // if formatted number occupies more than 2 characters,
        // drop the parentheses to make it fit
        if runtime.bak < -99 {
            write!(&mut acc, "{}", runtime.bak);
        } else {
            write!(&mut bak, "({})", runtime.bak);
        }

        let last = match runtime.last {
            Some(Dir::Up) => "UP",
            Some(Dir::Down) => "DOWN",
            Some(Dir::Left) => "LEFT",
            Some(Dir::Right) => "RIGHT",
            None => "N/A",
        };

        let mode = "TODO";

        Gizmos {
            acc,
            bak,
            last,
            mode,
        }
    }
}

enum ReadContinuation {
    Mov(ResolvedDst),
    Add,
    Sub,
    Jro,
}

pub enum ExecNodeIO<F>
where
    F: FnMut(Num) -> NodeOutbox,
{
    None,
    InDir(Dir, F),
    InAny(F),
    Out(NodeOutbox),
}

fn update_state(text: &NodeText) -> ExecNodeState {
    match parse_node_text(text) {
        Ok(code) if code.is_empty() => ExecNodeState::Empty,
        Ok(code) => ExecNodeState::Ready(code),
        Err(parse_err) => ExecNodeState::Errored(parse_err),
    }
}

fn validate_text_dimensions(node_text: &NodeText) -> bool {
    node_text
        .split('\n')
        .all(|line| line.len() <= consts::NODE_LINE_LENGTH)
        && node_text.split('\n').count() <= consts::NODE_LINES
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

type NodeCode<Label = u8> = ArrayVec<Instruction<Label>, { consts::NODE_LINES }>;

#[derive(Clone, Copy, Debug)]
struct Instruction<Label: Debug + Copy = u8> {
    op: Op<Label>,
    src_line: u8,
}

use std::{collections::HashMap, fmt::Debug};

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
    Any,
    Last,
}

#[derive(Clone, Copy, Debug)]
enum Dst {
    Dir(Dir),
    Acc,
    Nil,
    Any,
    Last,
}

#[derive(Clone, Debug)]
pub struct ParseErr {
    pub problem: ParseProblem,
    pub line: u8,
}

#[derive(Clone, Debug)]
pub enum ParseProblem {
    NotEnoughArgs,
    TooManyArgs,
    InvalidSrc,
    InvalidDst,
    InvalidInstruction,
    UndefinedLabel,
}

impl ParseProblem {
    pub fn to_str(&self) -> &'static str {
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
