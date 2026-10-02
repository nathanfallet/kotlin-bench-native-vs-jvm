//! Rust port of `src/commonMain/kotlin/bench/Micro.kt`.
//!
//! Every case reproduces the Kotlin data, arithmetic and random stream exactly, so the checksum must match
//! the Kotlin one. The code is written the way Rust is normally written: small structs are values
//! (no heap allocation), generics are monomorphised (no boxing), and the standard `HashMap`/`HashSet`
//! use their default SipHash hasher. The `*-boxed` cases force heap allocation to isolate the allocator.

use std::collections::{HashMap, HashSet};
use std::hint::black_box;
use std::time::Instant;

use crate::long_map::LongObjectMap;
use crate::util::{mix_hash, JavaRandom};
use crate::Options;

const OPERATIONS: usize = 1_000_000;

// ------------------------------------------------------------------ types used by the cases

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
struct BlockPos {
    x: i32,
    y: i32,
    z: i32,
}

impl BlockPos {
    fn offset(self, dx: i32, dy: i32, dz: i32) -> BlockPos {
        BlockPos { x: self.x + dx, y: self.y + dy, z: self.z + dz }
    }
}

#[derive(Clone, Copy)]
struct PackedPos(i64);

impl PackedPos {
    fn of(x: i32, y: i32, z: i32) -> PackedPos {
        PackedPos((((x as i64) & 0x3FFFFFF) << 38) | (((z as i64) & 0x3FFFFFF) << 12) | ((y as i64) & 0xFFF))
    }
    fn x(self) -> i32 {
        (self.0 >> 38) as i32
    }
    fn y(self) -> i32 {
        ((self.0 << 52) >> 52) as i32
    }
    fn z(self) -> i32 {
        ((self.0 << 26) >> 38) as i32
    }
    fn offset(self, dx: i32, dy: i32, dz: i32) -> PackedPos {
        PackedPos::of(self.x() + dx, self.y() + dy, self.z() + dz)
    }
}

/// The same seven block classes as the Kotlin registry, called through a trait object.
trait Block {
    fn is_solid(&self) -> bool {
        true
    }
}
struct PlainBlock;
struct AirBlock;
struct GrassBlock;
struct FallingBlock;
struct WaterBlock;
struct LeavesBlock;
struct CropBlock;
impl Block for PlainBlock {}
impl Block for GrassBlock {}
impl Block for FallingBlock {}
impl Block for LeavesBlock {}
impl Block for AirBlock {
    fn is_solid(&self) -> bool {
        false
    }
}
impl Block for WaterBlock {
    fn is_solid(&self) -> bool {
        false
    }
}
impl Block for CropBlock {
    fn is_solid(&self) -> bool {
        false
    }
}

/// Registry in Kotlin order: air, stone, dirt, grass, sand, gravel, water, log, leaves, wheat 0..7.
fn block_by_id(id: i32) -> &'static dyn Block {
    match id {
        0 => &AirBlock,
        1 | 2 | 7 => &PlainBlock,
        3 => &GrassBlock,
        4 | 5 => &FallingBlock,
        6 => &WaterBlock,
        8 => &LeavesBlock,
        _ => &CropBlock,
    }
}

const WHEAT_LAST_ID: i32 = 16;

trait TableReader {
    fn read(&self, index: usize) -> f64;
}

// `it * 0.25 - 2.0` for it in 0..16, written out: Rust statics are compile-time data, no initialisation check.
static TABLE: [f64; 16] = [
    -2.0, -1.75, -1.5, -1.25, -1.0, -0.75, -0.5, -0.25, 0.0, 0.25, 0.5, 0.75, 1.0, 1.25, 1.5, 1.75,
];

struct StaticTableReader;
impl TableReader for StaticTableReader {
    fn read(&self, index: usize) -> f64 {
        TABLE[index]
    }
}

struct FieldTableReader {
    table: [f64; 16],
}
impl TableReader for FieldTableReader {
    fn read(&self, index: usize) -> f64 {
        self.table[index]
    }
}

// ------------------------------------------------------------------ cases

type Body = Box<dyn FnMut() -> i64>;

fn case(name: &str, seed: i64) -> Option<(&'static str, Body)> {
    let seed_int = seed as i32;
    Some(match name {
        "alloc-temporary" => {
            let mut pos = BlockPos { x: seed_int & 1023, y: 64, z: 0 };
            ("BlockPos as a plain struct: a value on the stack, no allocation", Box::new(move || {
                let mut acc = 0i64;
                for i in 0..OPERATIONS {
                    let next = black_box(pos).offset(1, 0, -1);
                    acc += (next.x + next.z) as i64;
                    if i & 1023 == 0 {
                        pos = next;
                    }
                }
                acc
            }))
        }
        "alloc-temporary-boxed" => {
            let mut pos = Box::new(BlockPos { x: seed_int & 1023, y: 64, z: 0 });
            ("BlockPos forced onto the heap with Box, to measure the allocator itself", Box::new(move || {
                let mut acc = 0i64;
                for i in 0..OPERATIONS {
                    let next = black_box(Box::new(pos.offset(1, 0, -1)));
                    acc += (next.x + next.z) as i64;
                    if i & 1023 == 0 {
                        pos = next;
                    }
                }
                acc
            }))
        }
        "alloc-retained" => {
            let mut ring: Vec<Option<BlockPos>> = vec![None; 65536];
            let mut x = seed_int & 1023;
            ("ring of 64k BlockPos values, stored inline in the Vec", Box::new(move || {
                let mut acc = 0i64;
                for i in 0..OPERATIONS {
                    let pos = BlockPos { x, y: 64, z: i as i32 };
                    x += 1;
                    ring[i & 65535] = Some(pos);
                    acc += ring[(i + 1) & 65535].map_or(0, |p| p.y) as i64;
                }
                acc
            }))
        }
        "alloc-retained-boxed" => {
            let mut ring: Vec<Option<Box<BlockPos>>> = (0..65536).map(|_| None).collect();
            let mut x = seed_int & 1023;
            ("ring of 64k boxed BlockPos: one heap allocation and one free per operation", Box::new(move || {
                let mut acc = 0i64;
                for i in 0..OPERATIONS {
                    let pos = Box::new(BlockPos { x, y: 64, z: i as i32 });
                    x += 1;
                    ring[i & 65535] = Some(pos);
                    acc += ring[(i + 1) & 65535].as_ref().map_or(0, |p| p.y) as i64;
                }
                acc
            }))
        }
        "value-class-temporary" => {
            let mut pos = PackedPos::of(seed_int & 1023, 64, 0);
            ("position packed in an i64", Box::new(move || {
                let mut acc = 0i64;
                for i in 0..OPERATIONS {
                    let next = black_box(pos).offset(1, 0, -1);
                    acc += (next.x() + next.z()) as i64;
                    if i & 1023 == 0 {
                        pos = next;
                    }
                }
                acc
            }))
        }
        "virtual-call-megamorphic" | "virtual-call-monomorphic" => {
            let blocks: Vec<&'static dyn Block> = if name == "virtual-call-megamorphic" {
                let mut random = JavaRandom::new(seed);
                (0..4096).map(|_| block_by_id(random.next_int(WHEAT_LAST_ID + 1))).collect()
            } else {
                (0..4096).map(|_| block_by_id(3)).collect()
            };
            ("dyn Block trait object call", Box::new(move || {
                let mut acc = 0i64;
                for i in 0..OPERATIONS {
                    if black_box(blocks[i & 4095]).is_solid() {
                        acc += 1;
                    }
                }
                acc
            }))
        }
        "companion-access-in-call" | "field-access-in-call" => {
            let mut random = JavaRandom::new(seed);
            let indices: Vec<usize> = (0..4096).map(|_| random.next_int(16) as usize).collect();
            let reader: Box<dyn TableReader> = if name == "companion-access-in-call" {
                Box::new(StaticTableReader)
            } else {
                Box::new(FieldTableReader { table: TABLE })
            };
            ("dyn call reading a static table / a field table", Box::new(move || {
                let mut acc = 0.0f64;
                for i in 0..OPERATIONS {
                    acc += black_box(&reader).read(indices[i & 4095]);
                }
                acc.to_bits() as i64
            }))
        }
        "list-iterator" | "list-indexed" => {
            let lists: Vec<Vec<i32>> =
                (0..1000).map(|i| (0..8).map(|it| (seed_int + i + it) & 1023).collect()).collect();
            let indexed = name == "list-indexed";
            ("Vec<i32>: unboxed elements", Box::new(move || {
                let mut acc = 0i64;
                for i in 0..OPERATIONS / 8 {
                    let list = black_box(&lists[i % 1000]);
                    if indexed {
                        for j in 0..list.len() {
                            acc += list[j] as i64;
                        }
                    } else {
                        for value in list {
                            acc += *value as i64;
                        }
                    }
                }
                acc
            }))
        }
        "lambda-pipeline" => {
            let lists: Vec<Vec<i32>> =
                (0..1000).map(|i| (0..8).map(|it| (seed_int + i * 7 + it * 13) & 1023).collect()).collect();
            ("iter().filter().min_by_key(): lazy, no intermediate list", Box::new(move || {
                let mut acc = 0i64;
                for i in 0..OPERATIONS / 8 {
                    let list = black_box(&lists[i % 1000]);
                    acc += list.iter().filter(|v| *v & 1 == 0).min_by_key(|v| *v & 63).copied().unwrap_or(0) as i64;
                }
                acc
            }))
        }
        "boxed-hashset-int" => {
            let base = (seed & 0xFFFF) as i32 + 100_000;
            ("std HashSet<i32> with the default SipHash hasher", Box::new(move || {
                let mut set = HashSet::new();
                let mut acc = 0i64;
                for i in 0..(OPERATIONS / 2) as i32 {
                    set.insert(base + (i & 4095));
                    if set.contains(&black_box(base + (i.wrapping_mul(7) & 8191))) {
                        acc += 1;
                    }
                }
                acc
            }))
        }
        "boxed-long-hashmap" => {
            let mut map: HashMap<i64, &'static str> = HashMap::new();
            for it in 0..4096i64 {
                map.insert(it * 31 + seed, "v");
            }
            ("std HashMap<i64, _> with the default SipHash hasher", Box::new(move || {
                let mut acc = 0i64;
                for i in 0..OPERATIONS {
                    if map.get(&black_box((i as i64 & 8191) * 31 + seed)).is_some() {
                        acc += 1;
                    }
                }
                acc
            }))
        }
        "primitive-long-map" => {
            let mut map = LongObjectMap::new(16);
            for it in 0..4096i64 {
                map.put(it * 31 + seed, "v");
            }
            ("same open-addressing map as the Kotlin LongObjectMap", Box::new(move || {
                let mut acc = 0i64;
                for i in 0..OPERATIONS {
                    if map.get(black_box((i as i64 & 8191) * 31 + seed)).is_some() {
                        acc += 1;
                    }
                }
                acc
            }))
        }
        "array-arithmetic" => {
            let a: Vec<i32> = (0..4096).map(|it: i32| (it * 31 + seed_int) & 1023).collect();
            let b: Vec<f64> = (0..4096).map(|it| it as f64 * 0.5).collect();
            ("same arithmetic over Vec<i32> and Vec<f64>", Box::new(move || {
                let mut acc = 0.0f64;
                for i in 0..OPERATIONS {
                    let j = i & 4095;
                    acc += a[j] as f64 * b[j] - (a[j] >> 3) as f64;
                }
                acc.to_bits() as i64
            }))
        }
        _ => return None,
    })
}

pub fn run(options: &Options) {
    let name = options.case.clone().expect("micro needs --case NAME");
    let warmup = options.warmup.unwrap_or(50);
    let iterations = options.iterations.unwrap_or(300);
    let seed = options.seed;
    let (description, mut body) = case(&name, seed).unwrap_or_else(|| panic!("unknown case {name}"));

    let mut samples = Vec::with_capacity(warmup + iterations);
    let mut checksum = 0i64;
    for _ in 0..warmup + iterations {
        let start = Instant::now();
        checksum = mix_hash(checksum, body());
        samples.push(start.elapsed().as_nanos() as i64);
    }
    let measured = &samples[warmup..];
    let mut sorted = measured.to_vec();
    sorted.sort_unstable();
    let percentile = |p: f64| sorted[((sorted.len() - 1) as f64 * p) as usize];
    let first: Vec<i64> = samples.iter().take(50).copied().collect();
    let total: i64 = measured.iter().sum();
    println!(
        "RESULT {{\"target\":\"rust-{}-{}\",\"workload\":\"micro:{}\",\"unit\":\"{} operations\",\"warmup\":{},\"iterations\":{},\
\"setupNs\":0,\"totalNs\":{},\"meanNs\":{},\"p50Ns\":{},\"p90Ns\":{},\"p99Ns\":{},\"maxNs\":{},\"first50MeanNs\":{},\
\"gcCount\":0,\"gcMillis\":0,\"gcPauseMillis\":0,\"checksum\":\"{:x}\",\"description\":\"{}\",\"operations\":\"{}\"}}",
        std::env::consts::OS,
        std::env::consts::ARCH,
        name,
        OPERATIONS,
        warmup,
        measured.len(),
        total,
        total / measured.len() as i64,
        percentile(0.5),
        percentile(0.9),
        percentile(0.99),
        sorted[sorted.len() - 1],
        first.iter().sum::<i64>() / first.len() as i64,
        checksum as u64,
        description,
        OPERATIONS,
    );
}
