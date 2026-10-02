//! Port of `Benchmarks.kt`: the tick, noise, collections and worldgen workloads.

use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;
use std::time::Instant;

use crate::entities::EntityType;
use crate::level::{Level, PHASES};
use crate::noise::PerlinOctaves;
use crate::result::{available_processors, sample_durations, BenchResult};
use crate::terrain::{Chunk, TerrainGenerator};
use crate::util::{mix_hash, JavaRandom};
use crate::Options;

/// Main workload: a Minecraft-like server tick over a 24x24-chunk world. One sample = one tick.
pub fn tick(options: &Options) -> BenchResult {
    let start = Instant::now();
    let mut level = create_level(options);
    let setup = start.elapsed().as_nanos() as i64;
    let warmup = options.warmup.unwrap_or(400);
    let iterations = options.iterations.unwrap_or(2000);
    let trace = options.trace;
    let mut peak_entities = 0;
    let samples = sample_durations(warmup, iterations, |index| {
        if index == warmup {
            level.phase_nanos = [0; PHASES.len()];
        }
        level.tick();
        peak_entities = peak_entities.max(level.entities.len());
        if trace {
            eprintln!("TRACE {} {:x} {}", level.game_time, level.checksum as u64, level.entities.len());
        }
    });
    let mut extra = vec![
        ("chunks".to_string(), level.chunks.len().to_string()),
        ("entitiesAtEnd".to_string(), level.entities.len().to_string()),
        ("peakEntities".to_string(), peak_entities.to_string()),
    ];
    for (i, name) in PHASES.iter().enumerate() {
        extra.push((format!("phase.{name}.meanNs"), (level.phase_nanos[i] / iterations as i64).to_string()));
    }
    BenchResult {
        workload: "tick".into(),
        unit: "tick".into(),
        warmup,
        samples,
        checksum: level.final_checksum(),
        setup_nanos: setup,
        extra,
    }
}

pub fn create_level(options: &Options) -> Level {
    let mut level = Level::new(options.seed, 24);
    let generator = TerrainGenerator::new(options.seed);
    for x in 0..level.size {
        for z in 0..level.size {
            level.chunks.put(Chunk::key(x, z), generator.generate(x, z));
        }
    }
    let target = |base: i32| (base as f64 * options.scale) as i32;
    level.population_targets[EntityType::Zombie as usize] = Some(target(1200));
    level.population_targets[EntityType::Cow as usize] = Some(target(1000));
    level.population_targets[EntityType::Villager as usize] = Some(target(700));
    level.population_targets[EntityType::Skeleton as usize] = Some(target(400));
    for _ in 0..20 {
        let pos = level.random_surface_position(32);
        let player = level.new_player();
        level.set_pos(player, pos.x as f64 + 0.5, pos.y as f64, pos.z as f64 + 0.5);
    }
    level.tick(); // spawns the initial population
    level
}

/// Control workload: octave Perlin noise over a 32^3 grid. Pure floating-point arithmetic, zero allocation.
/// `noise-hoisted` runs the same code (see `noise.rs`).
pub fn noise(options: &Options, hoisted: bool) -> BenchResult {
    let noise = PerlinOctaves::new(&mut JavaRandom::new(options.seed), 6);
    let warmup = options.warmup.unwrap_or(30);
    let iterations = options.iterations.unwrap_or(200);
    let mut checksum = 0i64;
    let samples = sample_durations(warmup, iterations, |iteration| {
        let mut sum = 0.0;
        for x in 0..32 {
            for y in 0..32 {
                for z in 0..32 {
                    sum += noise.sample((x + iteration as i32 * 32) as f64 / 64.0, y as f64 / 64.0, z as f64 / 64.0);
                }
            }
        }
        checksum = mix_hash(checksum, sum.to_bits() as i64);
    });
    BenchResult {
        workload: if hoisted { "noise-hoisted" } else { "noise" }.into(),
        unit: "32^3 samples x 6 octaves".into(),
        warmup,
        samples,
        checksum,
        setup_nanos: 0,
        extra: Vec::new(),
    }
}

/// Idiomatic Rust collections: std `HashMap<i64, Payload>` churn, `Vec<i32>`, sort, iterator pipeline, group-by.
/// `Payload` is stored inline in the map; nothing is boxed.
pub fn collections(options: &Options) -> BenchResult {
    struct Payload {
        a: i32,
        b: i32,
    }
    let mut random = JavaRandom::new(options.seed);
    let mut map: HashMap<i64, Payload> = HashMap::new();
    let warmup = options.warmup.unwrap_or(30);
    let iterations = options.iterations.unwrap_or(300);
    let mut checksum = 0i64;
    let samples = sample_durations(warmup, iterations, |_| {
        for _ in 0..50_000 {
            let key = random.next_int(200_000) as i64 * 31;
            if map.remove(&key).is_none() {
                map.insert(key, Payload { a: key as i32, b: random.next_int(1000) });
            }
        }
        let mut sum: i64 = map.values().filter(|p| p.b % 3 == 0).map(|p| p.a as i64 * p.b as i64).sum();
        let mut list: Vec<i32> = (0..20_000).map(|_| random.next_int(1_000_000)).collect();
        list.sort_unstable();
        sum += list[list.len() / 2] as i64;
        // `groupBy`: each group keeps insertion order, so its first element is the first one inserted.
        let mut groups: HashMap<i32, Vec<i32>> = HashMap::new();
        for &value in &list {
            groups.entry(value % 16).or_default().push(value);
        }
        sum += groups.values().map(|group| group.len() as i64 * group[0] as i64).sum::<i64>();
        checksum = mix_hash(checksum, sum);
    });
    BenchResult {
        workload: "collections".into(),
        unit: "iteration".into(),
        warmup,
        samples,
        checksum,
        setup_nanos: 0,
        extra: Vec::new(),
    }
}

/// Chunk generation in batches of 64 chunks. `--threads 1` generates sequentially; otherwise the batch is spread
/// over N scoped threads that pull chunk indices from a shared counter (dynamic scheduling like a work queue).
/// The hashes are combined in chunk order, like Kotlin's `awaitAll`.
pub fn worldgen(options: &Options) -> BenchResult {
    let generator = TerrainGenerator::new(options.seed);
    let threads = options.threads.unwrap_or_else(available_processors);
    let warmup = options.warmup.unwrap_or(5);
    let iterations = options.iterations.unwrap_or(40);
    let mut checksum = 0i64;
    let mut batch = 0;
    let samples = sample_durations(warmup, iterations, |_| {
        let base = batch * 8;
        batch += 1;
        let generate = |i: i32| generator.generate(base + i / 8, i % 8).content_hash();
        let hashes: Vec<i64> = if threads == 1 {
            (0..64).map(generate).collect()
        } else {
            let next = AtomicUsize::new(0);
            let hashes = Mutex::new([0i64; 64]);
            std::thread::scope(|scope| {
                for _ in 0..threads {
                    scope.spawn(|| loop {
                        let i = next.fetch_add(1, Ordering::Relaxed);
                        if i >= 64 {
                            break;
                        }
                        let hash = generate(i as i32);
                        hashes.lock().unwrap()[i] = hash;
                    });
                }
            });
            hashes.into_inner().unwrap().to_vec()
        };
        for hash in hashes {
            checksum = mix_hash(checksum, hash);
        }
    });
    BenchResult {
        workload: if threads == 1 { "worldgen-1-thread".into() } else { format!("worldgen-{threads}-threads") },
        unit: "batch of 64 chunks".into(),
        warmup,
        samples,
        checksum,
        setup_nanos: 0,
        extra: Vec::new(),
    }
}
