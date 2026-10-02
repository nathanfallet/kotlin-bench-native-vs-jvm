//! Port of `Geometry.kt`. Every type here is a small `Copy` value: where Kotlin allocates a new `BlockPos`,
//! `Vec3` or `AABB` on the heap for every neighbour lookup or movement step, Rust keeps them in registers.

use std::ops::Add;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct BlockPos {
    pub x: i32,
    pub y: i32,
    pub z: i32,
}

impl BlockPos {
    #[inline]
    pub const fn new(x: i32, y: i32, z: i32) -> Self {
        BlockPos { x, y, z }
    }
    #[inline]
    pub fn above(self) -> Self {
        BlockPos::new(self.x, self.y + 1, self.z)
    }
    #[inline]
    pub fn below(self) -> Self {
        BlockPos::new(self.x, self.y - 1, self.z)
    }
    #[inline]
    pub fn offset(self, dx: i32, dy: i32, dz: i32) -> Self {
        BlockPos::new(self.x + dx, self.y + dy, self.z + dz)
    }
    #[inline]
    pub fn relative(self, direction: Direction) -> Self {
        let (dx, dy, dz) = direction.delta();
        self.offset(dx, dy, dz)
    }
    #[inline]
    pub fn as_long(self) -> i64 {
        ((self.x as i64 & 0x3FFFFFF) << 38) | ((self.z as i64 & 0x3FFFFFF) << 12) | (self.y as i64 & 0xFFF)
    }
    #[inline]
    pub fn dist_sqr(self, other: BlockPos) -> i32 {
        let dx = self.x - other.x;
        let dy = self.y - other.y;
        let dz = self.z - other.z;
        dx * dx + dy * dy + dz * dz
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Direction {
    Down,
    Up,
    North,
    South,
    West,
    East,
}

impl Direction {
    /// Same order as Kotlin's `Direction.entries`.
    pub const ALL: [Direction; 6] =
        [Direction::Down, Direction::Up, Direction::North, Direction::South, Direction::West, Direction::East];
    pub const HORIZONTAL: [Direction; 4] = [Direction::North, Direction::South, Direction::West, Direction::East];

    #[inline]
    pub const fn delta(self) -> (i32, i32, i32) {
        match self {
            Direction::Down => (0, -1, 0),
            Direction::Up => (0, 1, 0),
            Direction::North => (0, 0, -1),
            Direction::South => (0, 0, 1),
            Direction::West => (-1, 0, 0),
            Direction::East => (1, 0, 0),
        }
    }
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Vec3 {
    pub x: f64,
    pub y: f64,
    pub z: f64,
}

impl Vec3 {
    pub const ZERO: Vec3 = Vec3 { x: 0.0, y: 0.0, z: 0.0 };

    #[inline]
    pub const fn new(x: f64, y: f64, z: f64) -> Self {
        Vec3 { x, y, z }
    }
    #[inline]
    pub fn scale(self, f: f64) -> Self {
        Vec3::new(self.x * f, self.y * f, self.z * f)
    }
    #[inline]
    pub fn length(self) -> f64 {
        (self.x * self.x + self.y * self.y + self.z * self.z).sqrt()
    }
    #[inline]
    pub fn normalize(self) -> Self {
        let length = self.length();
        if length < 1.0E-4 {
            Vec3::ZERO
        } else {
            Vec3::new(self.x / length, self.y / length, self.z / length)
        }
    }
}

impl Add for Vec3 {
    type Output = Vec3;
    #[inline]
    fn add(self, o: Vec3) -> Vec3 {
        Vec3::new(self.x + o.x, self.y + o.y, self.z + o.z)
    }
}

/// Axis-aligned bounding box, with the three `clip*Collide` sweeps vanilla uses to resolve movement.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Aabb {
    pub min_x: f64,
    pub min_y: f64,
    pub min_z: f64,
    pub max_x: f64,
    pub max_y: f64,
    pub max_z: f64,
}

impl Aabb {
    pub const EMPTY: Aabb = Aabb::new(0.0, 0.0, 0.0, 0.0, 0.0, 0.0);

    #[inline]
    pub const fn new(min_x: f64, min_y: f64, min_z: f64, max_x: f64, max_y: f64, max_z: f64) -> Self {
        Aabb { min_x, min_y, min_z, max_x, max_y, max_z }
    }

    #[inline]
    pub fn of_block(x: i32, y: i32, z: i32) -> Self {
        Aabb::new(x as f64, y as f64, z as f64, x as f64 + 1.0, y as f64 + 1.0, z as f64 + 1.0)
    }

    #[inline]
    pub fn move_by(self, dx: f64, dy: f64, dz: f64) -> Self {
        Aabb::new(self.min_x + dx, self.min_y + dy, self.min_z + dz, self.max_x + dx, self.max_y + dy, self.max_z + dz)
    }

    #[inline]
    pub fn inflate(self, x: f64, y: f64, z: f64) -> Self {
        Aabb::new(self.min_x - x, self.min_y - y, self.min_z - z, self.max_x + x, self.max_y + y, self.max_z + z)
    }

    #[inline]
    pub fn inflate_all(self, amount: f64) -> Self {
        self.inflate(amount, amount, amount)
    }

    #[inline]
    pub fn expand_towards(self, dx: f64, dy: f64, dz: f64) -> Self {
        Aabb::new(
            if dx < 0.0 { self.min_x + dx } else { self.min_x },
            if dy < 0.0 { self.min_y + dy } else { self.min_y },
            if dz < 0.0 { self.min_z + dz } else { self.min_z },
            if dx > 0.0 { self.max_x + dx } else { self.max_x },
            if dy > 0.0 { self.max_y + dy } else { self.max_y },
            if dz > 0.0 { self.max_z + dz } else { self.max_z },
        )
    }

    #[inline]
    pub fn intersects(&self, o: &Aabb) -> bool {
        self.min_x < o.max_x
            && self.max_x > o.min_x
            && self.min_y < o.max_y
            && self.max_y > o.min_y
            && self.min_z < o.max_z
            && self.max_z > o.min_z
    }

    // In the three sweeps `self` is the obstacle and `other` the moving box; no signed zero or NaN can reach
    // `min`/`max` here, so `f64::min`/`max` agree with `kotlin.math.min`/`max`.
    #[inline]
    pub fn clip_x_collide(&self, other: &Aabb, dx: f64) -> f64 {
        if other.max_y <= self.min_y || other.min_y >= self.max_y || other.max_z <= self.min_z || other.min_z >= self.max_z {
            return dx;
        }
        if dx > 0.0 && other.max_x <= self.min_x {
            dx.min(self.min_x - other.max_x)
        } else if dx < 0.0 && other.min_x >= self.max_x {
            dx.max(self.max_x - other.min_x)
        } else {
            dx
        }
    }

    #[inline]
    pub fn clip_y_collide(&self, other: &Aabb, dy: f64) -> f64 {
        if other.max_x <= self.min_x || other.min_x >= self.max_x || other.max_z <= self.min_z || other.min_z >= self.max_z {
            return dy;
        }
        if dy > 0.0 && other.max_y <= self.min_y {
            dy.min(self.min_y - other.max_y)
        } else if dy < 0.0 && other.min_y >= self.max_y {
            dy.max(self.max_y - other.min_y)
        } else {
            dy
        }
    }

    #[inline]
    pub fn clip_z_collide(&self, other: &Aabb, dz: f64) -> f64 {
        if other.max_x <= self.min_x || other.min_x >= self.max_x || other.max_y <= self.min_y || other.min_y >= self.max_y {
            return dz;
        }
        if dz > 0.0 && other.max_z <= self.min_z {
            dz.min(self.min_z - other.max_z)
        } else if dz < 0.0 && other.min_z >= self.max_z {
            dz.max(self.max_z - other.min_z)
        } else {
            dz
        }
    }
}
