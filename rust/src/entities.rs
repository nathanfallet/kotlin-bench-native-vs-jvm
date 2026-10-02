//! Port of `Entities.kt`.
//!
//! # Dispatch
//!
//! The Kotlin class hierarchy (`Entity` > `LivingEntity` > `Mob` > `Zombie`…) becomes one [`Entity`] struct with
//! the fields every entity has, plus an [`EntityData`] enum for the per-kind state. Behaviour is a `match` on
//! that enum (static dispatch over a closed set); the per-type constants (`maxHealth`, `moveSpeed`, size, goals)
//! are `match`es on [`EntityType`]. Goals are the [`GoalKind`] enum, each variant holding its own state inline,
//! and a mob's goals are a `Vec<Goal>` owned by the mob: no trait objects, no per-goal allocation, and the
//! selector bookkeeping uses bit masks instead of the temporary lists of the Kotlin pipeline.
//!
//! All the logic is on `Level`, taking [`EntityId`]s, because an entity's tick reads and writes other entities
//! and the world (see `level.rs` for the storage model). The evaluation order of every random draw, every
//! floating-point expression and every list is the same as in Kotlin.

use crate::blocks::Block;
use crate::geometry::{Aabb, BlockPos, Vec3};
use crate::level::{EntityId, Level, Player, NO_SECTION};
use crate::network::{Connection, MoveEntityPacket};
use crate::util::{fast_atan2, floor_int};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum EntityType {
    Zombie,
    Cow,
    Villager,
    Skeleton,
    Item,
    Arrow,
    Player,
}

impl EntityType {
    pub const COUNT: usize = 7;
    pub const ALL: [EntityType; Self::COUNT] = [
        EntityType::Zombie,
        EntityType::Cow,
        EntityType::Villager,
        EntityType::Skeleton,
        EntityType::Item,
        EntityType::Arrow,
        EntityType::Player,
    ];

    fn width(self) -> f64 {
        match self {
            EntityType::Zombie | EntityType::Villager | EntityType::Skeleton | EntityType::Player => 0.6,
            EntityType::Cow => 0.9,
            EntityType::Item => 0.25,
            EntityType::Arrow => 0.5,
        }
    }

    fn height(self) -> f64 {
        match self {
            EntityType::Zombie | EntityType::Villager => 1.95,
            EntityType::Cow => 1.4,
            EntityType::Skeleton => 1.99,
            EntityType::Item => 0.25,
            EntityType::Arrow => 0.5,
            EntityType::Player => 1.8,
        }
    }

    fn max_health(self) -> f32 {
        match self {
            EntityType::Cow => 10.0,
            _ => 20.0,
        }
    }

    fn move_speed(self) -> f64 {
        match self {
            EntityType::Zombie => 0.23,
            EntityType::Cow => 0.2,
            EntityType::Villager | EntityType::Player => 0.5,
            _ => 0.25,
        }
    }

    fn goals(self) -> Vec<Goal> {
        use GoalKind::*;
        match self {
            EntityType::Zombie => vec![
                Goal::new(2, TARGET, NearestTarget { targets: &[EntityType::Villager, EntityType::Cow, EntityType::Player] }),
                Goal::new(3, MOVE | LOOK, MeleeAttack { cooldown: 0 }),
                Goal::new(7, MOVE, RandomStroll { ticks: 0 }),
                Goal::new(8, LOOK, LookAtNearest { look_at: None, ticks: 0 }),
            ],
            EntityType::Skeleton => vec![
                Goal::new(2, TARGET, NearestTarget { targets: &[EntityType::Zombie, EntityType::Player] }),
                Goal::new(3, MOVE | LOOK, RangedAttack { cooldown: 0 }),
                Goal::new(7, MOVE, RandomStroll { ticks: 0 }),
                Goal::new(8, LOOK, LookAtNearest { look_at: None, ticks: 0 }),
            ],
            EntityType::Cow => vec![
                Goal::new(1, MOVE, Panic),
                Goal::new(6, MOVE, RandomStroll { ticks: 0 }),
                Goal::new(7, LOOK, LookAtNearest { look_at: None, ticks: 0 }),
            ],
            EntityType::Villager => vec![
                Goal::new(1, MOVE, Panic),
                Goal::new(3, MOVE, HarvestCrops { crop: None }),
                Goal::new(6, MOVE, RandomStroll { ticks: 0 }),
                Goal::new(8, LOOK, LookAtNearest { look_at: None, ticks: 0 }),
            ],
            _ => unreachable!("{self:?} has no goals"),
        }
    }
}

pub struct Living {
    health: f32,
    last_hurt_time: i64,
    navigation_target: Option<Vec3>,
}

impl Living {
    fn new() -> Self {
        Living { health: -1.0, last_hurt_time: -1000, navigation_target: None }
    }
}

pub struct MobData {
    living: Living,
    target: Option<EntityId>,
    goals: Vec<Goal>,
}

/// Per-kind state. `Mob` covers zombies, cows, villagers and skeletons, which differ only by their constants
/// and goals.
pub enum EntityData {
    Mob(MobData),
    Player { living: Living, inventory: i64 },
    Item { item_id: i32, count: i32 },
    Arrow { owner: Option<EntityId>, in_ground_ticks: i32 },
}

pub struct Entity {
    pub id: i32,
    pub ty: EntityType,
    pub x: f64,
    pub y: f64,
    pub z: f64,
    pub motion: Vec3,
    pub bb: Aabb,
    pub on_ground: bool,
    pub horizontal_collision: bool,
    pub removed: bool,
    pub age: i32,
    pub y_rot: f32,
    pub x_rot: f32,
    pub section_key: i64,
    sent_x: f64,
    sent_y: f64,
    sent_z: f64,
    pub tracking_delta: Option<MoveEntityPacket>,
    pub data: EntityData,
    /// Number of links (`target`, `look_at`, `owner`) pointing at this entity; see `level.rs`.
    pub(crate) pins: u32,
    /// Removed and out of the `entities`/pending lists: the slot is recycled once `pins` drops to zero.
    pub(crate) unlisted: bool,
}

impl Entity {
    pub fn new(id: i32, ty: EntityType, data: EntityData) -> Self {
        Entity {
            id,
            ty,
            x: 0.0,
            y: 0.0,
            z: 0.0,
            motion: Vec3::ZERO,
            bb: Aabb::EMPTY,
            on_ground: false,
            horizontal_collision: false,
            removed: false,
            age: 0,
            y_rot: 0.0,
            x_rot: 0.0,
            section_key: NO_SECTION,
            sent_x: 0.0,
            sent_y: 0.0,
            sent_z: 0.0,
            tracking_delta: None,
            data,
            pins: 0,
            unlisted: false,
        }
    }

    #[inline]
    pub fn is_living(&self) -> bool {
        matches!(self.data, EntityData::Mob(_) | EntityData::Player { .. })
    }

    #[inline]
    fn living(&self) -> &Living {
        match &self.data {
            EntityData::Mob(mob) => &mob.living,
            EntityData::Player { living, .. } => living,
            _ => unreachable!("not a living entity"),
        }
    }

    #[inline]
    fn living_mut(&mut self) -> &mut Living {
        match &mut self.data {
            EntityData::Mob(mob) => &mut mob.living,
            EntityData::Player { living, .. } => living,
            _ => unreachable!("not a living entity"),
        }
    }

    #[inline]
    fn mob(&self) -> &MobData {
        match &self.data {
            EntityData::Mob(mob) => mob,
            _ => unreachable!("not a mob"),
        }
    }

    #[inline]
    fn mob_mut(&mut self) -> &mut MobData {
        match &mut self.data {
            EntityData::Mob(mob) => mob,
            _ => unreachable!("not a mob"),
        }
    }

    #[inline]
    fn position(&self) -> Vec3 {
        Vec3::new(self.x, self.y, self.z)
    }

    #[inline]
    fn distance_to_sqr(&self, other: &Entity) -> f64 {
        let dx = self.x - other.x;
        let dy = self.y - other.y;
        let dz = self.z - other.z;
        dx * dx + dy * dy + dz * dz
    }

    pub fn prepare_tracking_delta(&mut self) {
        let dx = ((self.x - self.sent_x) * 4096.0) as i64;
        let dy = ((self.y - self.sent_y) * 4096.0) as i64;
        let dz = ((self.z - self.sent_z) * 4096.0) as i64;
        self.tracking_delta = if dx == 0 && dy == 0 && dz == 0 {
            None
        } else {
            // `toInt().toShort()` truncates, like `as i16`.
            Some(MoveEntityPacket {
                entity_id: self.id,
                dx: dx as i16,
                dy: dy as i16,
                dz: dz as i16,
                y_rot: angle(self.y_rot),
                x_rot: angle(self.x_rot),
                on_ground: self.on_ground,
            })
        };
    }

    pub fn commit_tracking_delta(&mut self) {
        if self.tracking_delta.is_none() {
            return;
        }
        self.sent_x = self.x;
        self.sent_y = self.y;
        self.sent_z = self.z;
    }

    /// Clears the outgoing links of an entity whose slot is being recycled and returns them.
    pub(crate) fn take_links(&mut self) -> [Option<EntityId>; 2] {
        match &mut self.data {
            EntityData::Mob(mob) => {
                let look_at = mob.goals.iter_mut().find_map(|goal| match &mut goal.kind {
                    GoalKind::LookAtNearest { look_at, .. } => look_at.take(),
                    _ => None,
                });
                mob.goals = Vec::new();
                [mob.target.take(), look_at]
            }
            EntityData::Arrow { owner, .. } => [owner.take(), None],
            _ => [None, None],
        }
    }
}

/// `(degrees * 256f / 360f).toInt().toByte()`: float arithmetic, saturating `toInt`, truncating `toByte`.
#[inline]
fn angle(degrees: f32) -> i8 {
    (degrees * 256f32 / 360f32) as i32 as i8
}

// ---------------------------------------------------------------- goals

const MOVE: u8 = 1;
const LOOK: u8 = 2;
const TARGET: u8 = 4;

pub struct Goal {
    priority: i32,
    flags: u8,
    running: bool,
    kind: GoalKind,
}

impl Goal {
    fn new(priority: i32, flags: u8, kind: GoalKind) -> Self {
        Goal { priority, flags, running: false, kind }
    }
}

/// One variant per Kotlin goal class, with that class's mutable state.
pub enum GoalKind {
    RandomStroll { ticks: i32 },
    LookAtNearest { look_at: Option<EntityId>, ticks: i32 },
    NearestTarget { targets: &'static [EntityType] },
    MeleeAttack { cooldown: i32 },
    RangedAttack { cooldown: i32 },
    Panic,
    HarvestCrops { crop: Option<BlockPos> },
}

impl Level {
    // ------------------------------------------------------------------ creation

    pub fn create_entity(&mut self, ty: EntityType) -> EntityId {
        match ty {
            EntityType::Zombie | EntityType::Cow | EntityType::Villager | EntityType::Skeleton => self.spawn(
                ty,
                EntityData::Mob(MobData { living: Living::new(), target: None, goals: ty.goals() }),
            ),
            EntityType::Item => self.new_item(Block::DIRT.id(), 1),
            EntityType::Arrow => self.new_arrow(None),
            EntityType::Player => panic!("players are created by the benchmark"),
        }
    }

    pub fn new_item(&mut self, item_id: i32, count: i32) -> EntityId {
        self.spawn(EntityType::Item, EntityData::Item { item_id, count })
    }

    fn new_arrow(&mut self, owner: Option<EntityId>) -> EntityId {
        self.relink(None, owner);
        self.spawn(EntityType::Arrow, EntityData::Arrow { owner, in_ground_ticks: -1 })
    }

    pub fn new_player(&mut self) -> EntityId {
        let entity = self.spawn(EntityType::Player, EntityData::Player { living: Living::new(), inventory: 0 });
        self.players.push(Player { entity, connection: Connection::new() });
        entity
    }

    // ------------------------------------------------------------------ Entity

    pub fn set_pos(&mut self, entity: EntityId, x: f64, y: f64, z: f64) {
        let e = &mut self.arena[entity];
        e.x = x;
        e.y = y;
        e.z = z;
        let half_width = e.ty.width() / 2.0;
        e.bb = Aabb::new(x - half_width, y, z - half_width, x + half_width, y + e.ty.height(), z + half_width);
        self.update_section(entity);
    }

    pub fn tick_entity(&mut self, entity: EntityId) {
        match self.arena[entity].data {
            EntityData::Mob(_) => self.tick_mob(entity),
            EntityData::Player { .. } => self.tick_player(entity),
            EntityData::Item { .. } => self.tick_item(entity),
            EntityData::Arrow { .. } => self.tick_arrow(entity),
        }
    }

    pub fn remove(&mut self, entity: EntityId) {
        if self.arena[entity].removed {
            return;
        }
        self.arena[entity].removed = true;
        self.on_removed(entity);
    }

    /// Moves with block collisions, resolving Y then X then Z like vanilla's `Entity.collide`.
    fn move_entity(&mut self, entity: EntityId, dx: f64, dy: f64, dz: f64) {
        let e = &self.arena[entity];
        let shapes = self.get_block_collisions(&e.bb.expand_towards(dx, dy, dz));
        let mut bbox = e.bb;
        let mut clipped_y = dy;
        for shape in &shapes {
            clipped_y = shape.clip_y_collide(&bbox, clipped_y);
        }
        bbox = bbox.move_by(0.0, clipped_y, 0.0);
        let mut clipped_x = dx;
        for shape in &shapes {
            clipped_x = shape.clip_x_collide(&bbox, clipped_x);
        }
        bbox = bbox.move_by(clipped_x, 0.0, 0.0);
        let mut clipped_z = dz;
        for shape in &shapes {
            clipped_z = shape.clip_z_collide(&bbox, clipped_z);
        }
        let (x, y, z) = (e.x, e.y, e.z);
        self.set_pos(entity, x + clipped_x, y + clipped_y, z + clipped_z);
        let e = &mut self.arena[entity];
        e.horizontal_collision = clipped_x != dx || clipped_z != dz;
        e.on_ground = clipped_y != dy && dy < 0.0;
        if clipped_y != dy {
            e.motion = Vec3::new(e.motion.x, 0.0, e.motion.z);
        }
    }

    // ------------------------------------------------------------------ LivingEntity

    fn tick_living_base(&mut self, entity: EntityId) {
        let e = &mut self.arena[entity];
        e.age += 1;
        let max_health = e.ty.max_health();
        let living = e.living_mut();
        if living.health < 0.0 {
            living.health = max_health;
        }
    }

    fn hurt(&mut self, entity: EntityId, amount: f32, attacker: Option<EntityId>) {
        if self.arena[entity].removed {
            return;
        }
        let attacker_pos = attacker.map(|a| (self.arena[a].x, self.arena[a].z));
        let game_time = self.game_time;
        let e = &mut self.arena[entity];
        let living = e.living_mut();
        living.health -= amount;
        living.last_hurt_time = game_time;
        let health = living.health;
        if let Some((ax, az)) = attacker_pos {
            let push = Vec3::new(e.x - ax, 0.0, e.z - az).normalize().scale(0.4);
            e.motion = Vec3::new(e.motion.x + push.x, 0.36, e.motion.z + push.z);
        }
        if health <= 0.0 {
            self.die(entity);
        }
    }

    fn die(&mut self, entity: EntityId) {
        for it in 0..1 + self.random.next_int(2) {
            let item = self.new_item(Block::DIRT.id() + it, 1);
            let e = &self.arena[entity];
            let (x, y, z) = (e.x, e.y + 0.5, e.z);
            self.set_pos(item, x, y, z);
            self.add_entity(item);
        }
        self.remove(entity);
    }

    /// Walks towards the navigation target, jumps over obstacles, applies gravity and friction.
    fn travel(&mut self, entity: EntityId) {
        let e = &mut self.arena[entity];
        let move_speed = e.ty.move_speed();
        let mut mx = e.motion.x;
        let mut mz = e.motion.z;
        let (x, z) = (e.x, e.z);
        if let Some(target) = e.living().navigation_target {
            let dx = target.x - x;
            let dz = target.z - z;
            let distance = (dx * dx + dz * dz).sqrt();
            if distance > 0.6 {
                mx += dx / distance * move_speed * 0.2;
                mz += dz / distance * move_speed * 0.2;
                e.y_rot = (fast_atan2(dz, dx) * 57.2957763671875) as f32 - 90f32;
            } else {
                e.living_mut().navigation_target = None;
            }
        }
        let mut my = e.motion.y - 0.08;
        if e.horizontal_collision && e.on_ground {
            my = 0.42;
        }
        e.motion = Vec3::new(mx, my, mz);
        self.move_entity(entity, mx, my, mz);
        let e = &mut self.arena[entity];
        let friction = if e.on_ground { 0.546 } else { 0.91 };
        e.motion = Vec3::new(e.motion.x * friction, e.motion.y * 0.98, e.motion.z * friction);
    }

    #[inline]
    fn set_navigation(&mut self, entity: EntityId, target: Option<Vec3>) {
        self.arena[entity].living_mut().navigation_target = target;
    }

    fn set_target(&mut self, mob: EntityId, target: Option<EntityId>) {
        let old = std::mem::replace(&mut self.arena[mob].mob_mut().target, target);
        self.relink(old, target);
    }

    /// `getEntities(...).minByOrNull { mob.distanceToSqr(it) }`: the first entity at the minimal distance.
    fn nearest(&self, mob: EntityId, bbox: Aabb, predicate: impl Fn(&Entity) -> bool) -> Option<EntityId> {
        let me = &self.arena[mob];
        let mut best: Option<(EntityId, f64)> = None;
        for candidate in self.get_entities(mob, bbox, predicate) {
            let distance = me.distance_to_sqr(&self.arena[candidate]);
            if best.map_or(true, |(_, min)| min > distance) {
                best = Some((candidate, distance));
            }
        }
        best.map(|(entity, _)| entity)
    }

    // ------------------------------------------------------------------ Mob and goal selector

    fn tick_mob(&mut self, mob: EntityId) {
        self.tick_living_base(mob);
        if let Some(target) = self.arena[mob].mob().target {
            if self.arena[target].removed {
                self.set_target(mob, None);
            }
        }
        let mut goals = std::mem::take(&mut self.arena[mob].mob_mut().goals);
        if self.arena[mob].age % 2 == 0 {
            self.update_goals(mob, &mut goals);
        } else {
            self.tick_running_goals(mob, &mut goals);
        }
        self.arena[mob].mob_mut().goals = goals;
        self.travel(mob);
    }

    /// Same decisions, in the same order, as the Kotlin `GoalSelector.update` pipeline.
    fn update_goals(&mut self, mob: EntityId, goals: &mut [Goal]) {
        // Every `canContinueToUse` is evaluated before any goal is stopped, like `filter { }.forEach { }`.
        let mut to_stop = 0u8;
        for (i, goal) in goals.iter_mut().enumerate() {
            if goal.running && !self.can_continue_to_use(mob, goal) {
                to_stop |= 1 << i;
            }
        }
        for (i, goal) in goals.iter_mut().enumerate() {
            if to_stop & (1 << i) != 0 {
                self.stop_goal(mob, goal);
                goal.running = false;
            }
        }
        // The lazy sequence: per goal, not running, not blocked by a running goal, then `canUse()`.
        let mut candidates = 0u8;
        for i in 0..goals.len() {
            let (flags, priority) = (goals[i].flags, goals[i].priority);
            if goals[i].running
                || goals.iter().any(|g| g.running && g.flags & flags != 0 && g.priority <= priority)
            {
                continue;
            }
            if self.can_use(mob, &mut goals[i]) {
                candidates |= 1 << i;
            }
        }
        for c in 0..goals.len() {
            if candidates & (1 << c) == 0 {
                continue;
            }
            let flags = goals[c].flags;
            for goal in goals.iter_mut() {
                if goal.running && goal.flags & flags != 0 {
                    self.stop_goal(mob, goal);
                    goal.running = false;
                }
            }
            self.start_goal(mob, &mut goals[c]);
            goals[c].running = true;
        }
        self.tick_running_goals(mob, goals);
    }

    fn tick_running_goals(&mut self, mob: EntityId, goals: &mut [Goal]) {
        for goal in goals.iter_mut() {
            if goal.running {
                self.tick_goal(mob, goal);
            }
        }
    }

    fn can_use(&mut self, mob: EntityId, goal: &mut Goal) -> bool {
        match &mut goal.kind {
            GoalKind::RandomStroll { .. } => {
                self.arena[mob].living().navigation_target.is_none() && self.random.next_int(120) == 0
            }
            GoalKind::LookAtNearest { look_at, .. } => {
                if self.random.next_float() >= 0.02f32 {
                    return false;
                }
                let found = self.nearest(mob, self.arena[mob].bb.inflate(8.0, 3.0, 8.0), Entity::is_living);
                let old = std::mem::replace(look_at, found);
                self.relink(old, found);
                found.is_some()
            }
            GoalKind::NearestTarget { targets } => {
                if self.arena[mob].mob().target.is_some() || self.random.next_int(10) != 0 {
                    return false;
                }
                let targets: &'static [EntityType] = *targets;
                let found = self.nearest(mob, self.arena[mob].bb.inflate(16.0, 4.0, 16.0), |e| targets.contains(&e.ty));
                self.set_target(mob, found);
                found.is_some()
            }
            GoalKind::MeleeAttack { .. } | GoalKind::RangedAttack { .. } => self.arena[mob].mob().target.is_some(),
            GoalKind::Panic => {
                let living = self.arena[mob].living();
                self.game_time - living.last_hurt_time < 60
            }
            GoalKind::HarvestCrops { crop } => {
                if self.random.next_int(40) != 0 {
                    return false;
                }
                let e = &self.arena[mob];
                let origin = BlockPos::new(floor_int(e.x), floor_int(e.y), floor_int(e.z));
                *crop = None;
                for dx in -6..=6 {
                    for dy in -1..=1 {
                        for dz in -6..=6 {
                            let pos = origin.offset(dx, dy, dz);
                            if self.get_block(pos) == Block::MATURE_WHEAT {
                                *crop = Some(pos);
                                return true;
                            }
                        }
                    }
                }
                false
            }
        }
    }

    fn can_continue_to_use(&mut self, mob: EntityId, goal: &mut Goal) -> bool {
        if matches!(goal.kind, GoalKind::MeleeAttack { .. } | GoalKind::RangedAttack { .. } | GoalKind::Panic) {
            // The default `canContinueToUse() = canUse()`; for these goals `canUse` has no side effect.
            return self.can_use(mob, goal);
        }
        match &goal.kind {
            GoalKind::RandomStroll { ticks } => self.arena[mob].living().navigation_target.is_some() && *ticks < 200,
            GoalKind::LookAtNearest { look_at, ticks } => {
                look_at.is_some_and(|other| !self.arena[other].removed) && *ticks < 60
            }
            GoalKind::NearestTarget { .. } => self.arena[mob].mob().target.is_some_and(|target| {
                !self.arena[target].removed && self.arena[mob].distance_to_sqr(&self.arena[target]) < 24.0 * 24.0
            }),
            GoalKind::HarvestCrops { crop } => crop.is_some_and(|pos| self.get_block(pos) == Block::MATURE_WHEAT),
            GoalKind::MeleeAttack { .. } | GoalKind::RangedAttack { .. } | GoalKind::Panic => unreachable!(),
        }
    }

    fn start_goal(&mut self, mob: EntityId, goal: &mut Goal) {
        match &mut goal.kind {
            GoalKind::RandomStroll { ticks } => {
                *ticks = 0;
                let limit = self.width_in_blocks() as f64 - 2.0;
                let (mx, mz) = (self.arena[mob].x, self.arena[mob].z);
                let x = (mx + self.random.next_int(21) as f64 - 10.0).clamp(2.0, limit);
                let z = (mz + self.random.next_int(21) as f64 - 10.0).clamp(2.0, limit);
                let y = self.surface_y(floor_int(x), floor_int(z)) as f64 + 1.0;
                self.set_navigation(mob, Some(Vec3::new(x, y, z)));
            }
            GoalKind::LookAtNearest { ticks, .. } => *ticks = 0,
            GoalKind::Panic => {
                let limit = self.width_in_blocks() as f64 - 2.0;
                let (mx, my, mz) = (self.arena[mob].x, self.arena[mob].y, self.arena[mob].z);
                let x = (mx + self.random.next_int(11) as f64 - 5.0).clamp(2.0, limit);
                let z = (mz + self.random.next_int(11) as f64 - 5.0).clamp(2.0, limit);
                self.set_navigation(mob, Some(Vec3::new(x, my, z)));
            }
            _ => {}
        }
    }

    fn stop_goal(&mut self, mob: EntityId, goal: &mut Goal) {
        match goal.kind {
            GoalKind::RandomStroll { .. }
            | GoalKind::MeleeAttack { .. }
            | GoalKind::RangedAttack { .. }
            | GoalKind::HarvestCrops { .. } => self.set_navigation(mob, None),
            GoalKind::NearestTarget { .. } => self.set_target(mob, None),
            GoalKind::LookAtNearest { .. } | GoalKind::Panic => {}
        }
    }

    fn tick_goal(&mut self, mob: EntityId, goal: &mut Goal) {
        match &mut goal.kind {
            GoalKind::RandomStroll { ticks } => *ticks += 1,
            GoalKind::LookAtNearest { look_at, ticks } => {
                *ticks += 1;
                let Some(other) = *look_at else { return };
                let (me, other) = (&self.arena[mob], &self.arena[other]);
                let y_rot = (fast_atan2(other.z - me.z, other.x - me.x) * 57.2957763671875) as f32 - 90f32;
                let x_rot = (fast_atan2(other.y - me.y, me.distance_to_sqr(other).sqrt()) * -57.2957763671875) as f32;
                let me = &mut self.arena[mob];
                me.y_rot = y_rot;
                me.x_rot = x_rot;
            }
            GoalKind::MeleeAttack { cooldown } => {
                let Some(target) = self.arena[mob].mob().target else { return };
                let position = self.arena[target].position();
                self.set_navigation(mob, Some(position));
                *cooldown -= 1;
                if *cooldown <= 0 && self.arena[mob].distance_to_sqr(&self.arena[target]) < 2.5 {
                    self.hurt(target, 3.0, Some(mob));
                    *cooldown = 20;
                }
            }
            GoalKind::RangedAttack { cooldown } => {
                let Some(target) = self.arena[mob].mob().target else { return };
                let distance_sqr = self.arena[mob].distance_to_sqr(&self.arena[target]);
                let navigation = (distance_sqr > 100.0).then(|| self.arena[target].position());
                self.set_navigation(mob, navigation);
                *cooldown -= 1;
                if *cooldown <= 0 && distance_sqr < 225.0 {
                    let arrow = self.new_arrow(Some(mob));
                    let (mx, my, mz) = (self.arena[mob].x, self.arena[mob].y, self.arena[mob].z);
                    self.set_pos(arrow, mx, my + 1.5, mz);
                    let t = &self.arena[target];
                    let aim = Vec3::new(t.x - mx, t.y + 1.0 - (my + 1.5), t.z - mz);
                    self.arena[arrow].motion = aim.normalize().scale(1.6) + Vec3::new(0.0, distance_sqr.sqrt() * 0.012, 0.0);
                    self.add_entity(arrow);
                    *cooldown = 40;
                }
            }
            GoalKind::HarvestCrops { crop } => {
                let Some(pos) = *crop else { return };
                self.set_navigation(mob, Some(Vec3::new(pos.x as f64 + 0.5, pos.y as f64, pos.z as f64 + 0.5)));
                let dx = pos.x as f64 + 0.5 - self.arena[mob].x;
                let dz = pos.z as f64 + 0.5 - self.arena[mob].z;
                if dx * dx + dz * dz < 2.0 {
                    self.set_block(pos, Block::wheat(0));
                    let item = self.new_item(Block::MATURE_WHEAT.id(), 2);
                    self.set_pos(item, pos.x as f64 + 0.5, pos.y as f64 + 0.5, pos.z as f64 + 0.5);
                    self.add_entity(item);
                    *crop = None;
                }
            }
            GoalKind::NearestTarget { .. } | GoalKind::Panic => {}
        }
    }

    // ------------------------------------------------------------------ items, arrows, players

    fn tick_item(&mut self, entity: EntityId) {
        let e = &mut self.arena[entity];
        e.age += 1;
        e.motion = Vec3::new(e.motion.x, e.motion.y - 0.04, e.motion.z);
        let motion = e.motion;
        self.move_entity(entity, motion.x, motion.y, motion.z);
        let e = &mut self.arena[entity];
        let friction = if e.on_ground { 0.588 } else { 0.98 };
        e.motion = Vec3::new(e.motion.x * friction, e.motion.y * 0.98, e.motion.z * friction);
        if e.age % 20 == 0 {
            let EntityData::Item { item_id, .. } = e.data else { unreachable!() };
            let search = e.bb.inflate(0.5, 0.0, 0.5);
            let merged = self.get_entities(entity, search, |other| {
                matches!(other.data, EntityData::Item { item_id: other_id, .. } if other_id == item_id)
            });
            for other in merged {
                let EntityData::Item { count: other_count, .. } = self.arena[other].data else { unreachable!() };
                if let EntityData::Item { count, .. } = &mut self.arena[entity].data {
                    *count += other_count;
                }
                self.remove(other);
            }
        }
        if self.arena[entity].age >= 1200 {
            self.remove(entity);
        }
    }

    fn tick_arrow(&mut self, entity: EntityId) {
        let e = &mut self.arena[entity];
        e.age += 1;
        let start = e.position();
        let EntityData::Arrow { owner, in_ground_ticks } = &mut e.data else { unreachable!() };
        let owner = *owner;
        if *in_ground_ticks >= 0 {
            *in_ground_ticks += 1;
            if *in_ground_ticks > 200 {
                self.remove(entity);
            }
            return;
        }
        let e = &self.arena[entity];
        let motion = e.motion;
        let end = start + motion;
        for step in 1..=4 {
            let point = start + motion.scale(step as f64 / 4.0);
            if self.get_block(BlockPos::new(floor_int(point.x), floor_int(point.y), floor_int(point.z))).is_solid() {
                self.set_pos(entity, point.x, point.y, point.z);
                if let EntityData::Arrow { in_ground_ticks, .. } = &mut self.arena[entity].data {
                    *in_ground_ticks = 0;
                }
                return;
            }
        }
        let search = self.arena[entity].bb.expand_towards(motion.x, motion.y, motion.z).inflate_all(0.3);
        // `it !== owner`: network ids are never reused and the owner is pinned, so comparing ids is identity.
        let owner_id = owner.map(|o| self.arena[o].id);
        let hit = self
            .get_entities(entity, search, |other| other.is_living() && Some(other.id) != owner_id)
            .first()
            .copied();
        if let Some(hit) = hit {
            self.hurt(hit, 4.0, owner);
            self.remove(entity);
            return;
        }
        self.set_pos(entity, end.x, end.y, end.z);
        let e = &mut self.arena[entity];
        e.motion = Vec3::new(e.motion.x * 0.99, e.motion.y * 0.99 - 0.05, e.motion.z * 0.99);
        if e.y < 0.0 || e.age > 400 {
            self.remove(entity);
        }
    }

    fn tick_player(&mut self, player: EntityId) {
        self.tick_living_base(player);
        let limit = self.width_in_blocks() as f64 - 4.0;
        let e = &mut self.arena[player];
        let max_health = e.ty.max_health();
        let living = e.living_mut();
        if living.health < max_health {
            living.health = max_health;
        }
        if e.age % 60 == 1 || e.living().navigation_target.is_none() {
            let (px, py, pz) = (e.x, e.y, e.z);
            let x = (px + self.random.next_int(33) as f64 - 16.0).clamp(4.0, limit);
            let z = (pz + self.random.next_int(33) as f64 - 16.0).clamp(4.0, limit);
            self.set_navigation(player, Some(Vec3::new(x, py, z)));
        }
        self.travel(player);
        let age = self.arena[player].age;
        if age % 20 == 0 {
            let e = &self.arena[player];
            let (fx, fy, fz) = (floor_int(e.x), floor_int(e.y), floor_int(e.z));
            let x = fx + self.random.next_int(7) - 3;
            let y = fy + self.random.next_int(4) - 2;
            let z = fz + self.random.next_int(7) - 3;
            let pos = BlockPos::new(x, y, z);
            let block = self.get_block(pos);
            if block != Block::AIR && block != Block::WATER {
                self.set_block(pos, Block::AIR);
                let item = self.new_item(block.id(), 1);
                self.set_pos(item, pos.x as f64 + 0.5, pos.y as f64 + 0.5, pos.z as f64 + 0.5);
                self.add_entity(item);
            }
        }
        if age % 30 == 0 {
            let e = &self.arena[player];
            let (fx, fy, fz) = (floor_int(e.x), floor_int(e.y), floor_int(e.z));
            let x = fx + self.random.next_int(5) - 2;
            let y = fy + self.random.next_int(3);
            let z = fz + self.random.next_int(5) - 2;
            let pos = BlockPos::new(x, y, z);
            if self.get_block(pos) == Block::AIR {
                let block = if self.random.next_boolean() { Block::SAND } else { Block::GRAVEL };
                self.set_block(pos, block);
            }
        }
        let picked = self.get_entities(player, self.arena[player].bb.inflate(1.0, 0.5, 1.0), |other| {
            matches!(other.data, EntityData::Item { .. }) && other.age > 10
        });
        for item in picked {
            let EntityData::Item { count, .. } = self.arena[item].data else { unreachable!() };
            if let EntityData::Player { inventory, .. } = &mut self.arena[player].data {
                *inventory += count as i64;
            }
            self.remove(item);
        }
    }
}
