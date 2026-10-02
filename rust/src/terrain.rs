//! Port of `Terrain.kt`: chunks and the terrain generator.

use crate::blocks::Block;
use crate::geometry::BlockPos;
use crate::noise::PerlinOctaves;
use crate::util::{mix_hash, JavaRandom};

pub const HEIGHT: i32 = 128;
pub const SEA_LEVEL: i32 = 56;

/// A 16 x HEIGHT x 16 column of block ids, stored flat like a vanilla palette-less section.
pub struct Chunk {
    pub x: i32,
    pub z: i32,
    blocks: Box<[i16]>,
}

impl Chunk {
    pub fn new(x: i32, z: i32) -> Self {
        Chunk { x, z, blocks: vec![0i16; (16 * 16 * HEIGHT) as usize].into_boxed_slice() }
    }

    #[inline]
    fn index(local_x: i32, y: i32, local_z: i32) -> usize {
        ((y << 8) | (local_z << 4) | local_x) as usize
    }

    #[inline]
    pub fn get(&self, local_x: i32, y: i32, local_z: i32) -> i32 {
        self.blocks[Self::index(local_x, y, local_z)] as i32
    }

    #[inline]
    pub fn set(&mut self, local_x: i32, y: i32, local_z: i32, id: i32) {
        self.blocks[Self::index(local_x, y, local_z)] = id as i16;
    }

    pub fn surface_y(&self, local_x: i32, local_z: i32) -> i32 {
        for y in (1..HEIGHT).rev() {
            if Block::by_id(self.get(local_x, y, local_z)).is_solid() {
                return y;
            }
        }
        0
    }

    pub fn content_hash(&self) -> i64 {
        let mut hash = mix_hash(0, ((self.x as i64) << 32) | (self.z as i64 & 0xFFFFFFFF));
        for &block in self.blocks.iter().step_by(7) {
            hash = mix_hash(hash, block as i64);
        }
        hash
    }

    #[inline]
    pub fn key(chunk_x: i32, chunk_z: i32) -> i64 {
        (chunk_x as i64 & 0xFFFFFFFF) | ((chunk_z as i64) << 32)
    }
}

/// Terrain with height noise, 3D cave noise, trees and wheat patches. Immutable after construction, so it is
/// `Sync` and shared by reference between worker threads.
pub struct TerrainGenerator {
    seed: i64,
    height_noise: PerlinOctaves,
    detail_noise: PerlinOctaves,
    cave_noise: PerlinOctaves,
}

impl TerrainGenerator {
    pub fn new(seed: i64) -> Self {
        let mut random = JavaRandom::new(seed);
        let height_noise = PerlinOctaves::new(&mut random, 4);
        let detail_noise = PerlinOctaves::new(&mut random, 2);
        let cave_noise = PerlinOctaves::new(&mut random, 3);
        TerrainGenerator { seed, height_noise, detail_noise, cave_noise }
    }

    pub fn generate(&self, chunk_x: i32, chunk_z: i32) -> Chunk {
        let mut chunk = Chunk::new(chunk_x, chunk_z);
        let mut random = JavaRandom::new(
            self.seed
                .wrapping_mul(341873128712)
                .wrapping_add((chunk_x as i64).wrapping_mul(132897987541))
                .wrapping_add(chunk_z as i64),
        );
        for local_x in 0..16 {
            for local_z in 0..16 {
                let world_x = chunk_x * 16 + local_x;
                let world_z = chunk_z * 16 + local_z;
                let height = ((SEA_LEVEL as f64
                    + self.height_noise.sample(world_x as f64 / 160.0, 0.0, world_z as f64 / 160.0) * 28.0
                    + self.detail_noise.sample(world_x as f64 / 24.0, 10.0, world_z as f64 / 24.0) * 4.0)
                    as i32)
                    .clamp(8, HEIGHT - 20);
                for y in 0..HEIGHT {
                    let mut block = if y == 0 || y < height - 3 {
                        Block::STONE
                    } else if y < height {
                        Block::DIRT
                    } else if y == height {
                        if height > SEA_LEVEL { Block::GRASS } else { Block::SAND }
                    } else if y <= SEA_LEVEL {
                        Block::WATER
                    } else {
                        Block::AIR
                    };
                    if (1..height - 1).contains(&y)
                        && self.cave_noise.sample(world_x as f64 / 40.0, y as f64 / 20.0, world_z as f64 / 40.0) > 0.42
                    {
                        block = Block::AIR;
                    }
                    chunk.set(local_x, y, local_z, block.id());
                }
            }
        }
        Self::place_trees(&mut chunk, &mut random);
        Self::place_wheat(&mut chunk, &mut random);
        chunk
    }

    fn place_trees(chunk: &mut Chunk, random: &mut JavaRandom) {
        for _ in 0..random.next_int(5) {
            let x = 2 + random.next_int(12);
            let z = 2 + random.next_int(12);
            let ground = chunk.surface_y(x, z);
            if chunk.get(x, ground, z) != Block::GRASS.id() || ground > HEIGHT - 12 {
                continue;
            }
            let trunk_height = 4 + random.next_int(3);
            let mut leaves = Vec::new();
            let top = BlockPos::new(x, ground + trunk_height, z);
            for dx in -2..=2 {
                for dy in -2..=1 {
                    for dz in -2..=2 {
                        let candidate = top.offset(dx, dy, dz);
                        if candidate.dist_sqr(top) <= 5 + random.next_int(2) {
                            leaves.push(candidate);
                        }
                    }
                }
            }
            for pos in leaves {
                if chunk.get(pos.x, pos.y, pos.z) == Block::AIR.id() {
                    chunk.set(pos.x, pos.y, pos.z, Block::LEAVES.id());
                }
            }
            for y in ground + 1..=ground + trunk_height {
                chunk.set(x, y, z, Block::LOG.id());
            }
        }
    }

    fn place_wheat(chunk: &mut Chunk, random: &mut JavaRandom) {
        if random.next_int(3) != 0 {
            return;
        }
        let start_x = random.next_int(10);
        let start_z = random.next_int(10);
        for x in start_x..start_x + 6 {
            for z in start_z..start_z + 6 {
                let ground = chunk.surface_y(x, z);
                if chunk.get(x, ground, z) == Block::GRASS.id() && chunk.get(x, ground + 1, z) == Block::AIR.id() {
                    chunk.set(x, ground, z, Block::DIRT.id());
                    chunk.set(x, ground + 1, z, Block::wheat(random.next_int(Block::MAX_STAGE + 1)).id());
                }
            }
        }
    }
}
