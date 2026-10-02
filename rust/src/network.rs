//! Port of `Network.kt`: packet buffer, packets and per-player tracking.

use std::collections::HashSet;

use crate::level::{EntityId, Level};
use crate::util::mix_hash;

/// Growable big-endian byte buffer with Minecraft's VarInt encoding.
pub struct PacketBuffer {
    data: Vec<u8>,
}

impl PacketBuffer {
    pub fn new(initial_capacity: usize) -> Self {
        PacketBuffer { data: Vec::with_capacity(initial_capacity) }
    }

    #[inline]
    pub fn len(&self) -> usize {
        self.data.len()
    }

    #[inline]
    pub fn write_byte(&mut self, value: i32) {
        self.data.push(value as u8);
    }

    pub fn write_var_int(&mut self, value: i32) {
        let mut v = value as u32;
        while v & !0x7F != 0 {
            self.write_byte(((v & 0x7F) | 0x80) as i32);
            v >>= 7;
        }
        self.write_byte(v as i32);
    }

    pub fn write_short(&mut self, value: i32) {
        self.write_byte(value >> 8);
        self.write_byte(value);
    }

    pub fn write_long(&mut self, value: i64) {
        self.data.extend_from_slice(&value.to_be_bytes());
    }

    pub fn write_double(&mut self, value: f64) {
        self.write_long(value.to_bits() as i64);
    }

    pub fn write_bytes(&mut self, other: &PacketBuffer) {
        self.data.extend_from_slice(&other.data);
    }

    /// Bytes are signed in Kotlin, so each one is sign-extended before mixing.
    pub fn hash(&self) -> i64 {
        self.data.iter().fold(-0x340d631b7bdddcdb, |hash, &b| mix_hash(hash, b as i8 as i64))
    }

    pub fn clear(&mut self) {
        self.data.clear();
    }
}

#[derive(Clone, Copy, Debug)]
pub struct MoveEntityPacket {
    pub entity_id: i32,
    pub dx: i16,
    pub dy: i16,
    pub dz: i16,
    pub y_rot: i8,
    pub x_rot: i8,
    pub on_ground: bool,
}

/// The Kotlin sealed class becomes an enum: packets are values, only `RemoveEntities` owns heap data.
pub enum Packet {
    AddEntity { entity_id: i32, entity_type: i32, x: f64, y: f64, z: f64 },
    MoveEntity(MoveEntityPacket),
    RemoveEntities(Vec<i32>),
    BlockUpdate { pos: i64, state: i32 },
}

impl Packet {
    fn id(&self) -> i32 {
        match self {
            Packet::AddEntity { .. } => 0x01,
            Packet::MoveEntity(_) => 0x2F,
            Packet::RemoveEntities(_) => 0x46,
            Packet::BlockUpdate { .. } => 0x09,
        }
    }

    fn write(&self, buffer: &mut PacketBuffer) {
        match self {
            Packet::AddEntity { entity_id, entity_type, x, y, z } => {
                buffer.write_var_int(*entity_id);
                buffer.write_var_int(*entity_type);
                buffer.write_double(*x);
                buffer.write_double(*y);
                buffer.write_double(*z);
            }
            Packet::MoveEntity(p) => {
                buffer.write_var_int(p.entity_id);
                buffer.write_short(p.dx as i32);
                buffer.write_short(p.dy as i32);
                buffer.write_short(p.dz as i32);
                buffer.write_byte(p.y_rot as i32);
                buffer.write_byte(p.x_rot as i32);
                buffer.write_byte(p.on_ground as i32);
            }
            Packet::RemoveEntities(ids) => {
                buffer.write_var_int(ids.len() as i32);
                for &id in ids {
                    buffer.write_var_int(id);
                }
            }
            Packet::BlockUpdate { pos, state } => {
                buffer.write_long(*pos);
                buffer.write_var_int(*state);
            }
        }
    }
}

const VIEW_DISTANCE: f64 = 48.0;
const BLOCK_UPDATE_DISTANCE_SQR: f64 = 64.0 * 64.0;

/// Per-player entity tracking and packet encoding. Owned by `Level` next to the player's entity id rather than
/// by the player entity, so the level can be borrowed immutably while a connection is updated.
pub struct Connection {
    tracked: HashSet<i32>,
    pending: Vec<Packet>,
    frame: PacketBuffer,
    scratch: PacketBuffer,
}

impl Connection {
    pub fn new() -> Self {
        Connection { tracked: HashSet::new(), pending: Vec::new(), frame: PacketBuffer::new(4096), scratch: PacketBuffer::new(256) }
    }

    pub fn send_changes(&mut self, level: &Level, player: EntityId) -> i64 {
        let me = &level.arena[player];
        let visible = level.get_entities(player, me.bb.inflate(VIEW_DISTANCE, VIEW_DISTANCE, VIEW_DISTANCE), |_| true);
        let mut visible_ids = HashSet::with_capacity(visible.len() * 2);
        for &handle in &visible {
            let entity = &level.arena[handle];
            visible_ids.insert(entity.id);
            if self.tracked.insert(entity.id) {
                self.pending.push(Packet::AddEntity {
                    entity_id: entity.id,
                    entity_type: entity.ty as i32,
                    x: entity.x,
                    y: entity.y,
                    z: entity.z,
                });
            } else if let Some(delta) = entity.tracking_delta {
                self.pending.push(Packet::MoveEntity(delta));
            }
        }
        let mut gone: Vec<i32> = self.tracked.iter().copied().filter(|id| !visible_ids.contains(id)).collect();
        if !gone.is_empty() {
            gone.sort_unstable();
            for id in &gone {
                self.tracked.remove(id);
            }
            self.pending.push(Packet::RemoveEntities(gone));
        }
        for &pos in &level.changed_blocks {
            let dx = pos.x as f64 - me.x;
            let dz = pos.z as f64 - me.z;
            if dx * dx + dz * dz < BLOCK_UPDATE_DISTANCE_SQR {
                self.pending.push(Packet::BlockUpdate { pos: pos.as_long(), state: level.get_block(pos).id() });
            }
        }
        self.flush()
    }

    /// Frames every pending packet as `length | id | payload`, the uncompressed vanilla wire format.
    fn flush(&mut self) -> i64 {
        for packet in self.pending.drain(..) {
            self.scratch.clear();
            self.scratch.write_var_int(packet.id());
            packet.write(&mut self.scratch);
            self.frame.write_var_int(self.scratch.len() as i32);
            self.frame.write_bytes(&self.scratch);
        }
        let hash = self.frame.hash();
        self.frame.clear();
        hash
    }
}
