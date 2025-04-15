use crate::num::Num;
use arrayvec::ArrayVec;

pub const INPUT_NODE_CAP: usize = 39;

#[derive(Clone, Debug)]
pub struct InputNode {
    data: ArrayVec<Num, INPUT_NODE_CAP>,
    pub index: Option<usize>,
}

impl InputNode {
    pub fn empty() -> Self {
        InputNode {
            data: ArrayVec::new(),
            index: None,
        }
    }

    pub fn with_data(data: ArrayVec<Num, INPUT_NODE_CAP>) -> Self {
        InputNode { data, index: None }
    }

    pub fn current(&self) -> Option<Num> {
        self.data.get(self.index?).copied()
    }
}
