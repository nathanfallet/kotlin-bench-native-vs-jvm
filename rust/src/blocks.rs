//! Port of `Blocks.kt`.
//!
//! Design choice: a block is its registry id (`Block(u16)`), and every behaviour is a `match` on that id,
//! i.e. static dispatch over a closed set. Kotlin models the registry as singleton objects with virtual
//! methods; since every Kotlin block is a stateless singleton compared by identity, comparing ids is the
//! same thing, and chunks store exactly these ids. The behaviours that act on the world (`random_tick`,
//! `tick`, `on_place`, `neighbor_changed`) live on `Level` in `level.rs`, because they need `&mut Level`.

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Block(pub u16);

impl Block {
    pub const AIR: Block = Block(0);
    pub const STONE: Block = Block(1);
    pub const DIRT: Block = Block(2);
    pub const GRASS: Block = Block(3);
    pub const SAND: Block = Block(4);
    pub const GRAVEL: Block = Block(5);
    pub const WATER: Block = Block(6);
    pub const LOG: Block = Block(7);
    pub const LEAVES: Block = Block(8);
    const WHEAT_0: u16 = 9;
    pub const MAX_STAGE: i32 = 7;
    pub const MATURE_WHEAT: Block = Block(Self::WHEAT_0 + Self::MAX_STAGE as u16);

    #[inline]
    pub fn by_id(id: i32) -> Block {
        Block(id as u16)
    }

    #[inline]
    pub fn id(self) -> i32 {
        self.0 as i32
    }

    #[inline]
    pub fn wheat(stage: i32) -> Block {
        Block(Self::WHEAT_0 + stage as u16)
    }

    /// Growth stage when this block is wheat.
    #[inline]
    pub fn wheat_stage(self) -> Option<i32> {
        (self.0 >= Self::WHEAT_0).then(|| (self.0 - Self::WHEAT_0) as i32)
    }

    #[inline]
    pub fn is_solid(self) -> bool {
        !(self == Block::AIR || self == Block::WATER || self.0 >= Self::WHEAT_0)
    }

    #[inline]
    pub fn is_randomly_ticking(self) -> bool {
        match self {
            Block::GRASS | Block::LEAVES => true,
            _ => matches!(self.wheat_stage(), Some(stage) if stage < Self::MAX_STAGE),
        }
    }
}
