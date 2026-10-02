//! Port of `Level.kt`, plus the block behaviours of `Blocks.kt` that act on the world.
//!
//! # Entity storage
//!
//! Kotlin entities are heap objects that point at each other (`Mob.target`, `LookAtNearestGoal.lookAt`,
//! `Arrow.owner`) and at the level. In Rust every entity lives by value in an [`Arena`] (a `Vec<Entity>` with a
//! free list) and everything else holds a copyable [`EntityId`], the slot index: the `entities` list, the
//! pending list, the section lists, the players and those three cross-entity links. All entity logic is written
//! as `Level` methods taking an id, so there are no shared references and no `Rc`/`RefCell`.
//!
//! The one thing the JVM heap gives Kotlin for free is that a removed entity stays readable while something
//! still refers to it: an arrow whose skeleton died still reads the skeleton's (frozen) position when it hits,
//! and a mob may read the position of the entity it was looking at one tick after that entity despawned. Both
//! affect the checksum. So a slot is only recycled once the entity is removed, out of the `entities`/pending
//! lists, *and* no link points at it: each slot counts the links pointing at it (`pins`). Removed entities are
//! already out of the section lists (`on_removed`), except removed players, which are never recycled.
//! This is deterministic, exact, and keeps memory bounded like the garbage-collected version.
//! `LivingEntity.lastAttacker` is written but never read in Kotlin, so it is not ported.

use std::ops::{Index, IndexMut};
use std::time::Instant;

use crate::blocks::Block;
use crate::entities::{Entity, EntityData, EntityType};
use crate::geometry::{Aabb, BlockPos, Direction};
use crate::long_map::LongObjectMap;
use crate::network::Connection;
use crate::terrain::{Chunk, HEIGHT, SEA_LEVEL};
use crate::util::{floor_int, mix_hash, JavaRandom};

/// Index of an entity slot in the [`Arena`]. Not the network id (`Entity::id`), which is never reused.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct EntityId(u32);

#[derive(Default)]
pub struct Arena {
    slots: Vec<Entity>,
    free: Vec<u32>,
}

impl Arena {
    fn insert(&mut self, entity: Entity) -> EntityId {
        match self.free.pop() {
            Some(slot) => {
                self.slots[slot as usize] = entity;
                EntityId(slot)
            }
            None => {
                self.slots.push(entity);
                EntityId(self.slots.len() as u32 - 1)
            }
        }
    }
}

impl Index<EntityId> for Arena {
    type Output = Entity;
    #[inline]
    fn index(&self, id: EntityId) -> &Entity {
        &self.slots[id.0 as usize]
    }
}

impl IndexMut<EntityId> for Arena {
    #[inline]
    fn index_mut(&mut self, id: EntityId) -> &mut Entity {
        &mut self.slots[id.0 as usize]
    }
}

#[derive(Clone, Copy)]
struct ScheduledTick {
    pos: BlockPos,
    block: Block,
    time: i64,
    order: i64,
}

/// Binary min-heap of scheduled block ticks, ordered by due time then insertion order, like vanilla's
/// `LevelTicks`. Same array heap as the Kotlin `TickQueue`, with the ticks stored inline.
struct TickQueue {
    heap: Vec<ScheduledTick>,
}

impl TickQueue {
    fn new() -> Self {
        TickQueue { heap: Vec::with_capacity(256) }
    }

    #[inline]
    fn before(a: &ScheduledTick, b: &ScheduledTick) -> bool {
        a.time < b.time || (a.time == b.time && a.order < b.order)
    }

    fn add(&mut self, tick: ScheduledTick) {
        let mut i = self.heap.len();
        self.heap.push(tick);
        while i > 0 {
            let parent = (i - 1) / 2;
            if !Self::before(&self.heap[i], &self.heap[parent]) {
                break;
            }
            self.heap.swap(i, parent);
            i = parent;
        }
    }

    fn peek(&self) -> Option<&ScheduledTick> {
        self.heap.first()
    }

    fn poll(&mut self) -> ScheduledTick {
        let top = self.heap.swap_remove(0);
        let size = self.heap.len();
        let mut i = 0;
        loop {
            let left = 2 * i + 1;
            if left >= size {
                break;
            }
            let right = left + 1;
            let child = if right < size && Self::before(&self.heap[right], &self.heap[left]) { right } else { left };
            if !Self::before(&self.heap[child], &self.heap[i]) {
                break;
            }
            self.heap.swap(i, child);
            i = child;
        }
        top
    }
}

pub struct Player {
    pub entity: EntityId,
    pub connection: Connection,
}

pub const PHASES: [&str; 6] = ["scheduledTicks", "randomTicks", "entities", "players", "trackingAndPackets", "respawn"];
pub const NO_SECTION: i64 = i64::MIN;
const RANDOM_TICK_SPEED: i32 = 3;

#[inline]
pub fn section_key(x: i32, y: i32, z: i32) -> i64 {
    ((x as i64 & 0x3FFFFF) << 42) | (y as i64 & 0xFFFFF) | ((z as i64 & 0x3FFFFF) << 20)
}

/// A square world of `size x size` chunks with walls at the border. One call to [`Level::tick`] runs the phases
/// of vanilla's `ServerLevel.tick`: scheduled ticks, random ticks, entities, then player tracking and packets.
pub struct Level {
    pub size: i32,
    pub random: JavaRandom,
    pub chunks: LongObjectMap<Chunk>,
    sections: LongObjectMap<Vec<EntityId>>,
    pub arena: Arena,
    pub entities: Vec<EntityId>,
    pending_entities: Vec<EntityId>,
    pub players: Vec<Player>,
    pub changed_blocks: Vec<BlockPos>,
    scheduled_ticks: TickQueue,
    tick_order: i64,
    pub game_time: i64,
    pub next_entity_id: i32,
    pub checksum: i64,
    /// Indexed by `EntityType` ordinal; `None` where the Kotlin `HashMap` has no entry.
    pub population_targets: [Option<i32>; EntityType::COUNT],
    population: [Option<i32>; EntityType::COUNT],
    /// Cumulated time per tick phase, indexed like [`PHASES`]. Reset by the benchmark after warm-up.
    pub phase_nanos: [i64; PHASES.len()],
}

impl Level {
    pub fn new(seed: i64, size: i32) -> Self {
        Level {
            size,
            random: JavaRandom::new(seed ^ 0x2545F4914F6CDD1D),
            chunks: LongObjectMap::new((size * size) as usize),
            sections: LongObjectMap::new(4096),
            arena: Arena::default(),
            entities: Vec::new(),
            pending_entities: Vec::new(),
            players: Vec::new(),
            changed_blocks: Vec::new(),
            scheduled_ticks: TickQueue::new(),
            tick_order: 0,
            game_time: 0,
            next_entity_id: 1,
            checksum: 0,
            population_targets: [None; EntityType::COUNT],
            population: [None; EntityType::COUNT],
            phase_nanos: [0; PHASES.len()],
        }
    }

    #[inline]
    pub fn width_in_blocks(&self) -> i32 {
        self.size * 16
    }

    // ------------------------------------------------------------------ blocks

    #[inline]
    pub fn get_block(&self, pos: BlockPos) -> Block {
        if pos.y < 0 || pos.y >= HEIGHT {
            return Block::AIR;
        }
        match self.chunks.get(Chunk::key(pos.x >> 4, pos.z >> 4)) {
            Some(chunk) => Block::by_id(chunk.get(pos.x & 15, pos.y, pos.z & 15)),
            None => Block::STONE,
        }
    }

    pub fn set_block(&mut self, pos: BlockPos, block: Block) {
        if pos.y <= 0 || pos.y >= HEIGHT {
            return;
        }
        let Some(chunk) = self.chunks.get_mut(Chunk::key(pos.x >> 4, pos.z >> 4)) else { return };
        if chunk.get(pos.x & 15, pos.y, pos.z & 15) == block.id() {
            return;
        }
        chunk.set(pos.x & 15, pos.y, pos.z & 15, block.id());
        self.changed_blocks.push(pos);
        self.on_place(block, pos);
        for direction in Direction::ALL {
            let neighbor = pos.relative(direction);
            let neighbor_block = self.get_block(neighbor);
            self.neighbor_changed(neighbor_block, neighbor);
        }
    }

    pub fn surface_y(&self, x: i32, z: i32) -> i32 {
        match self.chunks.get(Chunk::key(x >> 4, z >> 4)) {
            Some(chunk) => chunk.surface_y(x & 15, z & 15),
            None => SEA_LEVEL,
        }
    }

    pub fn schedule_tick(&mut self, pos: BlockPos, block: Block, delay: i32) {
        let tick = ScheduledTick { pos, block, time: self.game_time + delay as i64, order: self.tick_order };
        self.tick_order += 1;
        self.scheduled_ticks.add(tick);
    }

    fn on_place(&mut self, block: Block, pos: BlockPos) {
        match block {
            Block::SAND | Block::GRAVEL => self.schedule_tick(pos, block, 2),
            Block::WATER => self.schedule_tick(pos, block, 5),
            _ => {}
        }
    }

    fn neighbor_changed(&mut self, block: Block, pos: BlockPos) {
        // Same behaviour as `on_place` for every block type of this benchmark.
        self.on_place(block, pos);
    }

    fn block_tick(&mut self, block: Block, pos: BlockPos) {
        match block {
            // Sand and gravel fall one block per tick.
            Block::SAND | Block::GRAVEL => {
                let below = pos.below();
                if below.y > 0 && !self.get_block(below).is_solid() {
                    self.set_block(pos, Block::AIR);
                    self.set_block(below, block);
                }
            }
            // Water flows down into air, and sideways over solid ground below sea level.
            Block::WATER => {
                let below = pos.below();
                if below.y > 0 && self.get_block(below) == Block::AIR {
                    self.set_block(below, block);
                    return;
                }
                if pos.y > SEA_LEVEL {
                    return;
                }
                for direction in Direction::HORIZONTAL {
                    let side = pos.relative(direction);
                    if self.get_block(side) == Block::AIR && self.get_block(side.below()).is_solid() {
                        self.set_block(side, block);
                    }
                }
            }
            _ => {}
        }
    }

    fn random_tick(&mut self, block: Block, pos: BlockPos) {
        match block {
            Block::GRASS => {
                if self.get_block(pos.above()).is_solid() {
                    self.set_block(pos, Block::DIRT);
                    return;
                }
                for _ in 0..4 {
                    let dx = self.random.next_int(3) - 1;
                    let dy = self.random.next_int(5) - 3;
                    let dz = self.random.next_int(3) - 1;
                    let target = pos.offset(dx, dy, dz);
                    if self.get_block(target) == Block::DIRT && !self.get_block(target.above()).is_solid() {
                        self.set_block(target, Block::GRASS);
                    }
                }
            }
            // Leaves decay when no log is within two blocks, dropping an item.
            Block::LEAVES => {
                if self.random.next_int(8) != 0 {
                    return;
                }
                for dx in -2..=2 {
                    for dy in -2..=2 {
                        for dz in -2..=2 {
                            if self.get_block(pos.offset(dx, dy, dz)) == Block::LOG {
                                return;
                            }
                        }
                    }
                }
                self.set_block(pos, Block::AIR);
                let item = self.new_item(block.id(), 1);
                self.set_pos(item, pos.x as f64 + 0.5, pos.y as f64 + 0.5, pos.z as f64 + 0.5);
                self.add_entity(item);
            }
            _ => {
                if let Some(stage) = block.wheat_stage() {
                    if self.random.next_int(3) == 0 {
                        self.set_block(pos, Block::wheat(stage + 1));
                    }
                }
            }
        }
    }

    // ------------------------------------------------------------------ entity storage

    pub(crate) fn spawn(&mut self, ty: EntityType, data: EntityData) -> EntityId {
        let id = self.next_entity_id;
        self.next_entity_id += 1;
        self.arena.insert(Entity::new(id, ty, data))
    }

    pub fn add_entity(&mut self, entity: EntityId) {
        self.pending_entities.push(entity);
        let count = &mut self.population[self.arena[entity].ty as usize];
        *count = Some(count.unwrap_or(0) + 1);
    }

    pub(crate) fn on_removed(&mut self, entity: EntityId) {
        let (key, ty) = (self.arena[entity].section_key, self.arena[entity].ty);
        if let Some(list) = self.sections.get_mut(key) {
            remove_first(list, entity);
        }
        let count = &mut self.population[ty as usize];
        *count = Some(count.unwrap_or(1) - 1);
    }

    pub(crate) fn update_section(&mut self, entity: EntityId) {
        let e = &self.arena[entity];
        let key = section_key(floor_int(e.x) >> 4, floor_int(e.y) >> 4, floor_int(e.z) >> 4);
        let old = e.section_key;
        if key == old {
            return;
        }
        if old != NO_SECTION {
            if let Some(list) = self.sections.get_mut(old) {
                remove_first(list, entity);
            }
        }
        self.sections.get_or_insert_with(key, Vec::new).push(entity);
        self.arena[entity].section_key = key;
    }

    /// Replaces a link (`old` by `new`), keeping the pin counts of both targets up to date.
    pub(crate) fn relink(&mut self, old: Option<EntityId>, new: Option<EntityId>) {
        if let Some(new) = new {
            self.arena[new].pins += 1;
        }
        if let Some(old) = old {
            let e = &mut self.arena[old];
            e.pins -= 1;
            if e.pins == 0 && e.unlisted {
                self.free(old);
            }
        }
    }

    /// Called when a removed entity leaves the `entities` or pending list for good.
    fn unlist(&mut self, entity: EntityId) {
        let e = &mut self.arena[entity];
        e.unlisted = true;
        if e.pins == 0 {
            self.free(entity);
        }
    }

    fn free(&mut self, entity: EntityId) {
        let links = self.arena[entity].take_links();
        self.arena.free.push(entity.0);
        for link in links.into_iter().flatten() {
            self.relink(Some(link), None);
        }
    }

    /// Entities whose box intersects `bbox`, found through the section map like `EntitySectionStorage`.
    pub fn get_entities(&self, except: EntityId, bbox: Aabb, predicate: impl Fn(&Entity) -> bool) -> Vec<EntityId> {
        let mut result = Vec::new();
        let min_x = floor_int(bbox.min_x - 2.0) >> 4;
        let max_x = floor_int(bbox.max_x + 2.0) >> 4;
        let min_y = floor_int(bbox.min_y - 4.0) >> 4;
        let max_y = floor_int(bbox.max_y + 2.0) >> 4;
        let min_z = floor_int(bbox.min_z - 2.0) >> 4;
        let max_z = floor_int(bbox.max_z + 2.0) >> 4;
        for sx in min_x..=max_x {
            for sz in min_z..=max_z {
                for sy in min_y..=max_y {
                    let Some(section) = self.sections.get(section_key(sx, sy, sz)) else { continue };
                    for &handle in section {
                        let entity = &self.arena[handle];
                        if handle != except && !entity.removed && entity.bb.intersects(&bbox) && predicate(entity) {
                            result.push(handle);
                        }
                    }
                }
            }
        }
        result
    }

    /// Solid block boxes overlapping `bbox`, like `getBlockCollisions`.
    pub fn get_block_collisions(&self, bbox: &Aabb) -> Vec<Aabb> {
        let mut result = Vec::new();
        for x in floor_int(bbox.min_x)..=floor_int(bbox.max_x) {
            for y in floor_int(bbox.min_y)..=floor_int(bbox.max_y) {
                for z in floor_int(bbox.min_z)..=floor_int(bbox.max_z) {
                    if self.get_block(BlockPos::new(x, y, z)).is_solid() {
                        result.push(Aabb::of_block(x, y, z));
                    }
                }
            }
        }
        result
    }

    pub fn random_surface_position(&mut self, margin: i32) -> BlockPos {
        let x = margin + self.random.next_int(self.width_in_blocks() - 2 * margin);
        let z = margin + self.random.next_int(self.width_in_blocks() - 2 * margin);
        BlockPos::new(x, self.surface_y(x, z) + 1, z)
    }

    // ------------------------------------------------------------------ tick

    #[inline]
    fn phase(&mut self, index: usize, body: impl FnOnce(&mut Self)) {
        let start = Instant::now();
        body(self);
        self.phase_nanos[index] += start.elapsed().as_nanos() as i64;
    }

    pub fn tick(&mut self) {
        self.game_time += 1;
        self.phase(0, Self::run_scheduled_ticks);
        self.phase(1, Self::random_ticks);
        self.phase(2, Self::tick_entities);
        self.phase(3, |level| {
            for i in 0..level.players.len() {
                let player = level.players[i].entity;
                level.tick_entity(player);
            }
        });
        self.phase(4, |level| {
            for i in 0..level.entities.len() {
                let entity = level.entities[i];
                level.arena[entity].prepare_tracking_delta();
            }
            let mut players = std::mem::take(&mut level.players);
            for player in &mut players {
                let hash = player.connection.send_changes(level, player.entity);
                level.checksum = mix_hash(level.checksum, hash);
            }
            level.players = players;
            for i in 0..level.entities.len() {
                let entity = level.entities[i];
                level.arena[entity].commit_tracking_delta();
            }
            level.changed_blocks.clear();
        });
        self.phase(5, Self::respawn);
    }

    fn run_scheduled_ticks(&mut self) {
        let mut budget = 4096;
        while budget > 0 {
            budget -= 1;
            let Some(&next) = self.scheduled_ticks.peek() else { break };
            if next.time > self.game_time {
                break;
            }
            self.scheduled_ticks.poll();
            if self.get_block(next.pos) == next.block {
                self.block_tick(next.block, next.pos);
            }
        }
    }

    fn random_ticks(&mut self) {
        for slot in 0..self.chunks.slot_count() {
            let Some(chunk) = self.chunks.value_at(slot) else { continue };
            let (chunk_x, chunk_z) = (chunk.x, chunk.z);
            for section_y in 0..HEIGHT / 16 {
                for _ in 0..RANDOM_TICK_SPEED {
                    let local_x = self.random.next_int(16);
                    let y = section_y * 16 + self.random.next_int(16);
                    let local_z = self.random.next_int(16);
                    let chunk = self.chunks.value_at(slot).unwrap();
                    let block = Block::by_id(chunk.get(local_x, y, local_z));
                    if block.is_randomly_ticking() {
                        self.random_tick(block, BlockPos::new(chunk_x * 16 + local_x, y, chunk_z * 16 + local_z));
                    }
                }
            }
        }
    }

    fn tick_entities(&mut self) {
        for i in 0..self.entities.len() {
            let entity = self.entities[i];
            if !self.arena[entity].removed {
                self.tick_entity(entity);
            }
        }
        let mut entities = std::mem::take(&mut self.entities);
        let mut dropped = Vec::new();
        entities.retain(|&entity| {
            let keep = !self.arena[entity].removed;
            if !keep {
                dropped.push(entity);
            }
            keep
        });
        let pending = std::mem::take(&mut self.pending_entities);
        for &entity in &pending {
            if self.arena[entity].removed {
                dropped.push(entity);
            } else {
                entities.push(entity);
            }
        }
        self.entities = entities;
        self.pending_entities = pending;
        self.pending_entities.clear();
        for entity in dropped {
            self.unlist(entity);
        }
    }

    fn respawn(&mut self) {
        for ty in EntityType::ALL {
            let Some(target) = self.population_targets[ty as usize] else { continue };
            let mut missing = target - self.population[ty as usize].unwrap_or(0);
            while missing > 0 {
                missing -= 1;
                let pos = self.random_surface_position(4);
                let entity = self.create_entity(ty);
                self.set_pos(entity, pos.x as f64 + 0.5, pos.y as f64, pos.z as f64 + 0.5);
                self.add_entity(entity);
            }
        }
    }

    pub fn final_checksum(&self) -> i64 {
        let mut hash = self.checksum;
        for &entity in &self.entities {
            let e = &self.arena[entity];
            hash = mix_hash(hash, e.id as i64);
            hash = mix_hash(hash, e.x.to_bits() as i64);
            hash = mix_hash(hash, e.y.to_bits() as i64);
            hash = mix_hash(hash, e.z.to_bits() as i64);
        }
        for chunk in self.chunks.values() {
            hash = mix_hash(hash, chunk.content_hash());
        }
        hash
    }
}

/// Kotlin's `ArrayList.remove(element)`: removes the first occurrence and keeps the order of the rest.
#[inline]
fn remove_first(list: &mut Vec<EntityId>, entity: EntityId) {
    if let Some(index) = list.iter().position(|&e| e == entity) {
        list.remove(index);
    }
}
