use raylib::math::Vector2;

#[derive(PartialEq, Eq, Clone, Copy, Debug)]
pub enum Dir {
    Up,
    Down,
    Left,
    Right,
}

impl Dir {
    pub const ALL: [Self; 4] = [Dir::Up, Dir::Down, Dir::Left, Dir::Right];

    pub fn normalized(&self) -> Vector2 {
        match self {
            Dir::Up => Vector2::new(0.0, -1.0),
            Dir::Down => Vector2::new(0.0, 1.0),
            Dir::Left => Vector2::new(-1.0, 0.0),
            Dir::Right => Vector2::new(1.0, 0.0),
        }
    }

    pub fn inverse(&self) -> Self {
        match self {
            Dir::Up => Dir::Down,
            Dir::Down => Dir::Up,
            Dir::Left => Dir::Right,
            Dir::Right => Dir::Left,
        }
    }

    pub fn rotate_right(&self) -> Self {
        match self {
            Dir::Left => Dir::Up,
            Dir::Up => Dir::Right,
            Dir::Right => Dir::Down,
            Dir::Down => Dir::Left,
        }
    }
}
