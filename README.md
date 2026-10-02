# Kotlin/Native vs JVM vs Rust on a Minecraft-like server tick

Does compiling a Kotlin game server to native code make it faster than running it on the JVM? And how far is either from Rust?

This repository answers that question for one specific kind of code: a **Minecraft-like server tick**.

- The same Kotlin Multiplatform sources are compiled for the **JVM** and for **Kotlin/Native**.
- A **Rust** port of the same workloads is written the way Rust is normally written.
- Every workload computes a checksum. All three implementations produce **the same checksum for every workload**, so they provably do the same work.

## TL;DR

Measured on an Apple M1 Pro (10 cores, 32 GB) running macOS, JDK 25, Kotlin 2.4.20 and Rust 1.90. Linux arm64 figures come from Docker on the same machine.

| | JVM (HotSpot, JDK 25) | Kotlin/Native | Rust |
|---|---:|---:|---:|
| **Server tick**, ~12,000 entities, mean per tick | 5.8 ms | 11.2 ms (**1.9× slower**) | 2.3 ms (**2.5× faster**) |
| Server tick, ~38,000 entities | 21.8 ms | 40.2 ms (1.8× slower) | 8.5 ms (2.6× faster) |
| World generation, 64 chunks, 1 thread | 60 ms | 144 ms (2.4× slower) | 36 ms (1.7× faster) |
| World generation, 64 chunks, 10 threads | 8.4 ms | 20.2 ms (2.4× slower) | 4.7 ms (1.8× faster) |
| Peak memory during the tick run | 469 MiB | 126 MiB | 49 MiB |
| Start-up (process start to exit) | 51 ms | 8 ms | 6 ms |

What this means:

1. **For a long-running server that allocates many short-lived objects, the JVM is about 2× faster than Kotlin/Native.** It holds on macOS and on Linux (2.4× there, see [the Linux section](#linux-arm64-a-kotlinnative-code-generation-problem)).
2. **Kotlin/Native wins on memory (2.5–13× less depending on the workload) and start-up (6–15× faster), and is faster than the JVM for the first ~50 ticks**, before the JIT has warmed up.
3. **The gap is not "native is slow".** On pure arithmetic and on virtual calls, Kotlin/Native matches or beats the JVM. It loses on four specific mechanisms that the JVM's JIT optimises away: short-lived allocations, boxed keys in hash maps, `companion object` access and iterators. See [Why](#why-mechanism-by-mechanism).
4. **Local fixes help locally.** Moving a lookup table out of a `companion object` takes `noise` from 2.9× to 1.3× the JVM's time. But turning `BlockPos` into a `value class` barely changes the whole tick (−5% on native): the cost is spread over many mechanisms, not concentrated in one type. See [Does writing Kotlin for the native target help?](#does-writing-kotlin-for-the-native-target-help)
5. **Rust is 2–2.6× faster than the JVM on the tick because it does not allocate, not because it allocates faster.** Its heap allocation is actually the slowest of the three (15 ns per `Box` on macOS), and its default `HashMap` is 6× slower than the JVM's.

Full tables, including percentiles, GC figures and every repetition, are in [`results/SUMMARY.md`](results/SUMMARY.md). Raw data is in [`results/*.json`](results/).

## Results

### macOS arm64 (Apple M1 Pro)

Mean duration per iteration, median over 3 repetitions. The ratio is relative to the JVM: **above 1× is slower than the JVM**.

| Workload | JVM | Kotlin/Native | Rust | Native ÷ JVM | Rust ÷ JVM |
|---|---:|---:|---:|---:|---:|
| `tick` | 5.76 ms | 11.18 ms | 2.32 ms | **1.94×** | **0.40×** |
| `tick-3x-entities` | 21.76 ms | 40.23 ms | 8.45 ms | 1.85× | 0.39× |
| `noise` (straight port) | 3.29 ms | 9.49 ms | 2.17 ms | 2.88× | 0.66× |
| `noise-hoisted` | 3.60 ms | 4.60 ms | 2.16 ms | 1.28× | 0.60× |
| `collections` | 9.04 ms | 14.93 ms | 2.32 ms | 1.65× | 0.26× |
| `worldgen-1-thread` | 60.36 ms | 143.75 ms | 35.65 ms | 2.38× | 0.59× |
| `worldgen-all-threads` | 8.41 ms | 20.16 ms | 4.71 ms | 2.40× | 0.56× |

Tail latency and memory for the tick:

| `tick` | JVM | Kotlin/Native | Rust |
|---|---:|---:|---:|
| p50 | 5.58 ms | 11.16 ms | 2.26 ms |
| p99 | 8.11 ms | 13.70 ms | 3.30 ms |
| Worst tick | 12.41 ms | 17.52 ms | 10.23 ms |
| First 50 ticks (before JIT warm-up) | 7.26 ms | **3.66 ms** | 0.86 ms |
| Garbage collections during the run | 128 | 1,613 | – |
| Stop-the-world GC pauses, total | 184 ms | 120 ms | – |
| Peak resident memory | 469 MiB | 126 MiB | 49 MiB |

### Linux arm64 (Docker on the same machine)

| Workload | JVM | Kotlin/Native | Rust | Native ÷ JVM | Rust ÷ JVM |
|---|---:|---:|---:|---:|---:|
| `tick` | 5.97 ms | 14.17 ms | 2.80 ms | **2.37×** | **0.47×** |
| `tick-3x-entities` | 22.65 ms | 47.82 ms | 9.27 ms | 2.11× | 0.41× |
| `noise` | 3.21 ms | 10.44 ms | 2.64 ms | 3.25× | 0.82× |
| `noise-hoisted` | 3.53 ms | 6.31 ms | 2.63 ms | 1.79× | 0.75× |
| `collections` | 7.28 ms | 15.29 ms | 2.41 ms | 2.10× | 0.33× |
| `worldgen-1-thread` | 59.44 ms | 158.34 ms | 41.45 ms | 2.66× | 0.70× |
| `worldgen-all-threads` | 8.68 ms | 30.17 ms | 5.73 ms | 3.48× | 0.66× |
| Start-up | 43.8 ms | 2.9 ms | 1.7 ms | | |

Kotlin/Native is further behind on Linux arm64 than on macOS, for a reason specific to that target (see [below](#linux-arm64-a-kotlinnative-code-generation-problem)).

## Why: mechanism by mechanism

### Where the tick spends its time

Every tick phase is timed separately (macOS, mean per tick):

| Phase | JVM | Kotlin/Native | Rust | Native ÷ JVM | Rust ÷ JVM |
|---|---:|---:|---:|---:|---:|
| Entities: AI, movement, collisions | 3.54 ms | 7.32 ms | 1.48 ms | 2.07× | 0.42× |
| Player tracking and packet encoding | 1.75 ms | 3.41 ms | 0.67 ms | 1.95× | 0.38× |
| Random block ticks | 0.40 ms | 0.38 ms | 0.15 ms | **0.94×** | 0.38× |
| Players, scheduled ticks, respawn | < 0.1 ms | < 0.1 ms | < 0.02 ms | | |

Kotlin/Native wins the only phase that is plain loops over arrays, with no allocation. It loses the phases that allocate, iterate collections and make virtual calls.

### The mechanisms, isolated

Each micro-benchmark performs one million operations of a single mechanism (macOS, nanoseconds per operation):

| Mechanism | JVM | Kotlin/Native | Rust | Native ÷ JVM |
|---|---:|---:|---:|---:|
| Short-lived object (`BlockPos.offset()` that dies at once) | 2.00 | 6.10 | 0.64 (a stack value) | **3.0×** |
| Object kept alive a while (64k-slot ring) | 2.68 | 13.51 | 1.03 (inline in a `Vec`) | **5.1×** |
| Same position as a `value class` packed in a `Long` | 0.88 | 1.28 | 1.25 | 1.5× |
| Heap allocation forced with `Box` (Rust only) | | | 15.13 | |
| `HashMap<Long, _>` lookup, boxed keys | 1.62 | 12.34 | 9.88 (SipHash) | **7.6×** |
| `HashSet<Int>` add and contains | 2.53 | 12.95 | 7.77 (SipHash) | **5.1×** |
| fastutil-style `Long → V` open-addressing map | 2.79 | 8.30 | 2.28 | 3.0× |
| `for (x in arrayList)` (iterator per loop) | 1.13 | 4.59 | 0.79 | **4.1×** |
| Indexed loop over an `ArrayList<Int>` | 1.17 | 3.94 | 0.79 | 3.4× |
| Small method reading a `companion object` table | 0.94 | 2.87 | 0.95 (a `static`) | **3.0×** |
| Same method reading an instance field | 0.94 | 0.96 | 0.96 | 1.0× |
| Virtual call, one receiver class | 0.50 | 0.56 | 0.96 | 1.1× |
| Virtual call, 7 receiver classes | 7.72 | 5.44 | 5.38 | **0.70×** |
| `filter { }` then `minByOrNull { }` on 8 elements | 18.89 | 17.21 | 1.06 (lazy iterator) | **0.91×** |
| Arithmetic over primitive arrays | 0.96 | 1.04 | 0.95 | 1.1× |

#### Allocation: the JVM removes most of it, Kotlin/Native cannot

- HotSpot's C2 compiler runs escape analysis. An object that never leaves the method (a `BlockPos` computed and read at once) is **not allocated at all**: its fields live in registers. Kotlin/Native compiles ahead of time and allocates every such object on the heap.
- When objects do survive, the JVM allocates by bumping a pointer in a thread-local buffer, and its generational GC only scans the young objects. The Kotlin/Native GC is not generational: every cycle marks the whole live heap, then sweeps it.
- During the tick, Kotlin/Native ran 1,613 GC cycles versus the JVM's 128. Its GC is concurrent, so it barely pauses the program (120 ms of pauses in total, less than the JVM's 184 ms). But its collection cycles added up to 16 seconds of work on the GC thread, and allocation itself runs on the main thread.
- Rust wins by not allocating: positions, vectors and boxes are plain `Copy` values. When Rust does allocate (`Box`), it is the slowest of the three, at 15 ns per allocation with macOS's allocator.

#### Boxing: generic collections of primitives

- `HashMap<Long, V>` and `HashSet<Int>` store every key as an object. The JVM largely hides it: small `Integer` values are cached, and escape analysis removes many temporary boxes.
- Kotlin/Native's `HashMap` is Kotlin's own implementation, not `java.util.HashMap`. It boxes every key, and calls `hashCode` and `equals` through bridges. That costs 5–7.6× the JVM.
- Rust stores `i64` keys unboxed, but its default hasher (SipHash, designed to resist collision attacks) is slow. Its `HashMap` ends up 6× slower than the JVM's: Rust projects that care swap the hasher.

#### `companion object` and `object` access: an initialisation check on every read

- Kotlin/Native initialises `object` and `companion object` lazily and thread-safely, so each access from a function that has not already done it checks the initialisation state. On macOS that check goes through `_tlv_get_addr`, the thread-local variable accessor, which is a real function call.
- A straight Java → Kotlin port turns every `static final` table into a `companion object` property. This is why `noise` is 2.9× slower than the JVM, and only 1.3× once the tables are hoisted into fields (`noise-hoisted`).
- The same check is inside the standard library. `ArrayList` iterators call `AbstractList.Companion.checkElementIndex`, and our own `LongObjectMap` keeps its hash function in a `companion object`: 24% of its lookup time is `_tlv_get_addr`.

#### Virtual calls: a tie, or native ahead

- One receiver class: the JVM inlines the call after profiling it; Kotlin/Native and Rust cannot. The cost is about the same because the call is cheap.
- Seven receiver classes: the JVM's inline cache gives up, and a plain vtable call (Kotlin/Native, Rust `dyn`) is faster.

#### Pure computation: a tie

Arithmetic over primitive arrays runs at the same speed on all three (0.95–1.04 ns). The JVM's advantage is not better code for plain loops.

### What the JVM's JIT is worth

The same tick and micro-benchmarks, on the JVM with its optimisations disabled one by one (macOS):

| Workload | Default JVM | Without escape analysis<br>`-XX:-DoEscapeAnalysis` | C1 only, no C2<br>`-XX:TieredStopAtLevel=1` | Kotlin/Native |
|---|---:|---:|---:|---:|
| `tick` (ms per tick) | 5.76 | 6.51 (+13%) | 9.72 (+69%) | 11.18 |
| Short-lived object (ns/op) | 2.00 | 2.41 | 2.34 | 6.10 |
| Object kept alive (ns/op) | 2.68 | 2.82 | 4.06 | 13.51 |
| `HashMap<Long, _>` lookup (ns/op) | 1.62 | 5.14 (3.2×) | 9.28 (5.7×) | 12.34 |
| `for (x in arrayList)` (ns/op) | 1.13 | 1.47 | 8.72 (7.7×) | 4.59 |
| Virtual call, one class (ns/op) | 0.50 | 0.50 | 0.75 | 0.56 |
| Virtual call, 7 classes (ns/op) | 7.72 | 7.60 | 8.10 | 5.44 |
| `filter` + `minByOrNull` (ns/op) | 18.89 | 6.05 (0.32×) | 16.55 | 17.21 |

- **Without its optimising compiler, the JVM falls to roughly Kotlin/Native's level on the tick** (9.7 ms vs 11.2 ms). Most of the JVM's advantage comes from C2: speculative inlining, devirtualisation, escape analysis and loop optimisations driven by runtime profiles. An ahead-of-time compiler such as Kotlin/Native's cannot make those bets.
- Escape analysis alone is worth 13% on the tick, and most of the JVM's lead on boxed hash maps.
- One result is unexplained: `filter` + `minByOrNull` runs 3× faster with escape analysis *disabled*. It is reported as measured.

Raw data: [`results/experiments/`](results/experiments/), 2 repetitions each.

### Profile of the native tick

Sampled with macOS `sample` for 15 s during a Kotlin/Native `tick` run. Breakdown of the main thread's busy time ([raw profile](results/analysis/)):

| Bucket | Share |
|---|---:|
| Game code (`bench.*`, including inlined collection code) | 57.1% |
| `kotlin.collections` (HashMap, ArrayList, iterators) | 21.9% |
| Thread-local access (`_tlv_get_addr`: object initialisation checks, runtime state) | 11.7% |
| Allocation | 6.4% |
| Boxing and `equals` bridges | 1.3% |
| Lazy / global initialisation | 0.8% |
| GC sweep on the main thread | 0.4% |

The GC runs on its own thread, busy 21% of the time: 55% marking, 33% sweeping. It costs a core next to the program, but it is not on the tick's critical path.

## Linux arm64: a Kotlin/Native code generation problem

On Linux arm64, Kotlin/Native is further behind than on macOS. Even plain arithmetic over arrays is 2.35× slower than the JVM, against 1.08× on macOS on the same hardware. Rust and the JVM run that loop at the same speed on both systems, so the container is not the cause.

The disassembly shows why. In the `linuxArm64` binary, Kotlin/Native does not inline its runtime helpers into user code: every array read, every GC frame, every type check is a real function call. The table counts call sites in each release binary ([`scripts/count_runtime_calls.py`](scripts/count_runtime_calls.py)):

| Runtime helper | `macosArm64` | `linuxArm64` | `linuxX64` |
|---|---:|---:|---:|
| `EnterFrame` / `LeaveFrame` (GC root frames) | 0 / 0 | 2,407 / 2,477 | 0 / 0 |
| `Kotlin_Any_getTypeInfo` (type checks) | 0 | 1,866 | 0 |
| `AllocInstance` | 560 | 1,696 | 532 |
| Array `get`/`set` (all element types) | 0 | 2,288 | 0 |
| `Kotlin_math_floor` | 0 | 179 | 0 |

- **The inner loop of `array-arithmetic` shows it directly.** On macOS, it is a bounds check and a load. On `linuxArm64`, it is `bl Kotlin_IntArray_get` and `bl Kotlin_DoubleArray_get`.
- **The target CPU is not the cause.** Kotlin/Native compiles `linuxArm64` for `cortex-a57` by default, versus `apple-m1` on macOS. A build with `-Xoverride-konan-properties=targetCpu.linux_arm64=neoverse-n1` (`-PlinuxArm64Cpu=neoverse-n1`) ran within 1% of the default on every workload.
- **`linuxX64` inlines like macOS does.** A Linux x86-64 server should therefore look more like the macOS results, but it was not measured here: running x86-64 under emulation on an ARM Mac would not give meaningful numbers. Contributions of results from a real x86-64 Linux machine are welcome (`scripts/run-linux-docker.sh` with `TARGET=linuxX64`).

This looks like a toolchain issue in Kotlin/Native 2.4.20 rather than a design limit, and is worth reporting upstream.

## Does writing Kotlin for the native target help?

The main code is a straight port on purpose. To test whether a native-friendly rewrite helps, [`experiments/blockpos-value-class.patch`](experiments/blockpos-value-class.patch) turns `BlockPos` into a `@JvmInline value class` packed in a `Long`, with the same API. That is the change the `value-class-temporary` micro-benchmark suggests: 4.6× cheaper on native, 2.3× on the JVM. The checksums are unchanged.

| `tick` (macOS, ms per tick) | JVM | Kotlin/Native | Native ÷ JVM |
|---|---:|---:|---:|
| `data class BlockPos` (reference) | 5.76 | 11.18 | 1.94× |
| `value class BlockPos` | 5.67 (−2%) | 10.67 (−5%) | 1.88× |
| `tick-3x-entities`, reference | 21.76 | 40.23 | 1.85× |
| `tick-3x-entities`, `value class` | 23.07 (+6%) | 41.24 (+3%) | 1.79× |

**Almost no effect on the whole tick**, although the micro-benchmark gain is large. `BlockPos` is only a small part of the tick's cost:

- `Vec3` and `AABB` stay heap objects, because Kotlin value classes are limited to a single field.
- A value class is boxed again as soon as it goes into a generic collection or a nullable variable, which the tick does (`changedBlocks`, the harvest target).
- Iterators, lambdas and the `HashSet<Int>` of player tracking are unchanged.

The table-hoisting fix is different: it applies to a hot, self-contained function, which is why `noise-hoisted` gains 2.1× on native.

Closing the gap with Rust would take rewriting the tick the way the Rust port is written: entities in arrays, values instead of objects, no lambdas or iterators in hot paths. That rewrite is not part of this repository.

## When is Kotlin/Native worth it?

Based on these measurements:

- **Short-lived processes**: command-line tools, serverless functions, jobs that start on demand. It starts in 3–8 ms instead of 44–51 ms, and is at full speed from the first iteration.
- **Memory-bound deployments**: many small services per host, sidecars, small containers. It used 2.5–13× less memory than the JVM on every workload here.
- **I/O-bound services**, which mostly wait on the network or a database. The throughput gap measured here matters little when the CPU is idle.
- **Single-binary distribution** with no JRE to install.
- **Sharing code with iOS and other native targets**, which is the main reason Kotlin/Native exists.

It is not the right choice for a long-running, CPU-bound, allocation-heavy process like a game server tick, if the code is written in the usual JVM style. The JVM is about 2× faster there, and Rust about 5×.

## What is measured

All Kotlin workloads live in `src/commonMain` and use nothing but the Kotlin standard library and kotlinx.coroutines.

| Workload | What it does | Why it matters |
|---|---|---|
| `tick` | One server tick over a 24×24-chunk world, 128 blocks high, with 20 scripted players. The world holds ~3,300 mobs; dropped items bring the total to ~12,000 entities. | The main question. Shaped like vanilla's `ServerLevel.tick`: many short-lived objects, virtual calls, lambdas, maps keyed by `Long`. |
| `tick-3x-entities` | The same tick with three times more mobs (~38,000 entities at steady state). | More garbage and a bigger live heap. |
| `noise` | 6-octave Perlin noise over a 32³ grid. Pure floating-point arithmetic, zero allocation. | Control: the kind of code where native compilation should shine. |
| `noise-hoisted` | Same algorithm, with the lookup tables moved from a `companion object` into instance fields. | Isolates the `companion object` access cost. In Rust, identical to `noise`. |
| `collections` | `HashMap<Long, _>` churn, `ArrayList<Int>`, sorting, sequences and `groupBy`. | Idiomatic Kotlin with boxed keys and values, i.e. what ported code looks like without fastutil. |
| `worldgen-1-thread` | Generates batches of 64 chunks (height noise, 3D cave noise, trees, wheat) on one thread. | Heavy numeric work with some allocation. |
| `worldgen-all-threads` | The same batches spread over `Dispatchers.Default` (Rust: `std::thread::scope`). | Multi-threaded allocation and scheduling. |
| `micro` | One mechanism per case, one million operations each (see the table above). | Explains the results. |
| `startup` | Starts the process and exits. | Where native is expected to win. |

### Inside one `tick`

In the same order as vanilla, each tick runs:

1. **Scheduled block ticks**: falling sand and gravel, flowing water, from a binary heap.
2. **Random block ticks**: 3 per 16³ section per tick. Grass spreads, crops grow, leaves decay.
3. **Entities**:
   - zombies, skeletons, cows and villagers run a goal selector built on collection pipelines and lambdas;
   - they look for targets through a section map, walk, jump and collide with blocks through `AABB` sweeps;
   - skeletons shoot arrows, villagers harvest wheat, items merge and despawn.
4. **Players**: they walk around, break and place blocks, and pick items up.
5. **Tracking**: for each player, the entities entering, leaving and moving within view distance, plus the nearby block updates, are encoded into length-prefixed packets with VarInts. This is the uncompressed vanilla wire format.

Everything is deterministic:

- `java.util.Random` is re-implemented bit for bit, and the `atan2` is arithmetic-only.
- The chunk and entity-section maps are open-addressing `Long → V` maps modelled on fastutil, so their iteration order is identical on every runtime.
- The checksum covers every entity position, every chunk and every byte "sent" to players.

A side result: floating-point results were bit-identical between the JVM, Kotlin/Native and Rust, on macOS and Linux.

This is a model of a server, not a server. It is sized to be representative in shape: allocation rate, call patterns and data structures. It makes no claim to reproduce vanilla's exact cost per tick.

### The Rust port

`rust/` reimplements every workload with the same algorithms, the same random stream and the same data structures (open-addressing map, scheduled-tick heap, section map). Its checksums match Kotlin's. It is written idiomatically, which is the point of the comparison:

- `BlockPos`, `Vec3` and `Aabb` are `Copy` values, never heap-allocated.
- Entities live in an arena (a `Vec` with a free list) and refer to each other by index, instead of by pointer.
- Blocks, entity kinds and goals are `enum`s, so behaviour is a `match` rather than a virtual call.
- The goal selector computes the same decisions with bit masks instead of temporary lists, and iterator chains are lazy.
- It has no dependencies: the multi-threaded world generation uses `std::thread::scope`, and the hash maps use the standard library with its default hasher.

The design notes are in the module documentation of [`rust/src/level.rs`](rust/src/level.rs) and [`rust/src/entities.rs`](rust/src/entities.rs).

## How close is this to the real (Mojang) server?

The Kotlin code is a **straight port in spirit** of vanilla's Java style, not an optimised Kotlin rewrite. On the JVM, Kotlin compiles to the same kinds of bytecode as Java, so the JVM column shows how HotSpot runs vanilla-style code. It is not a measurement of vanilla itself.

| Aspect | Vanilla 26.x (Java) | This benchmark (Kotlin) | Effect on the comparison |
|---|---|---|---|
| Block positions | Immutable `BlockPos` allocated everywhere, plus `MutableBlockPos` reused in the hottest loops | Immutable `data class BlockPos` everywhere, no mutable variant | Slightly more allocation than vanilla, which hurts native more than the JVM |
| Vectors, bounding boxes | Immutable `Vec3`, `AABB` | Same | Same |
| Maps keyed by `Long` (chunks, sections) | fastutil `Long2ObjectOpenHashMap` | A port of it (`LongObjectMap`) | Same algorithm on every runtime |
| Collections elsewhere | `java.util` and fastutil | Kotlin collections: `java.util` on the JVM, Kotlin's own implementation on native | Native uses a different, slower `HashMap` implementation |
| Constant tables | `static final` arrays | `companion object` vals, what a Java → Kotlin port produces | Free on the JVM, costly on native; `noise-hoisted` shows the fix |
| Goal selection | Streams and lambdas | Collection pipelines and lambdas | Same style |
| `Random` | `LegacyRandomSource` / `java.util.Random` | Bit-exact clone of `java.util.Random` | Same |
| Block states | Paletted, bit-packed sections, thousands of states with properties | Flat `ShortArray`, a few dozen block types | Much less work per block access than vanilla |
| Entity AI | Full A* pathfinding, many goal types, brains | Straight-line navigation, a handful of goals | Much less work per entity than vanilla |
| Collisions | `VoxelShape`s of arbitrary complexity | Full-block boxes only | Less work than vanilla |
| Network | Netty, zlib compression, AES encryption | Hand-rolled buffers, no compression or encryption | Less work than vanilla |
| Value classes | Not applicable (Java has none yet) | **Not used** in the measured code, on purpose | Measured separately in `micro:value-class-temporary` and [`experiments/`](experiments/) |

Absolute tick times are therefore much lower than a real server's for the same entity count. The ratios between runtimes are what this benchmark is about.

Nothing here comes from Mojang's code: the model was written from the behaviour of the game, and the repository contains no decompiled source.

## Method

- **Setup**: Apple M1 Pro, 10 cores, 32 GB; macOS 26.5. Linux runs are in Docker on the same machine (linuxkit 6.10, 10 CPUs, 8 GB).
- **Toolchains**: Kotlin 2.4.20, kotlinx.coroutines 1.11.0, Rust 1.90.0. The JVM is OpenJDK 25 on macOS and Temurin 25.0.4 in the container.
- **Runtimes**:
  - JVM: a self-contained jar (`java -jar`) with default flags, i.e. G1 and a heap capped at a quarter of RAM.
  - Kotlin/Native: a `release` executable with the default garbage collector (concurrent mark and sweep).
  - Rust: `cargo build --release` with `lto = "fat"` and `codegen-units = 1`.
- **Processes**: each run is a separate process wrapped in `/usr/bin/time`, which records the peak resident set size of the whole process. Runtimes are interleaved, with 3 repetitions per workload, and tables report the median of each metric over the repetitions.
- **Warm-up**: each workload has a warm-up phase (600 ticks for `tick`) that is excluded from the statistics. The mean of the first 50 iterations is reported separately, so the JIT's warm-up cost stays visible.
- **GC figures**: JVM figures come from its management beans. Kotlin/Native only exposes its last collection, so it is polled after every iteration; its GC times are therefore a lower bound.

## Limitations

- **One machine, one CPU family.** Linux figures come from a VM on the same Mac; no x86-64 results.
- **A model, not a server.** See [How close is this to the real server](#how-close-is-this-to-the-real-mojang-server). Other code shapes will give other ratios.
- **Default settings everywhere.**
  - No JVM tuning. A fixed heap, ZGC or Aikar's flags might change the JVM column.
  - One Kotlin/Native GC experiment: fixing the GC target heap (`--native-heap-mb`) made collections more frequent and the tick slightly slower, so it is not used.
  - The Rust port keeps the standard SipHash hasher.
- **The Rust port is idiomatic by design.** It produces the same results with a different memory layout. It measures what the language gives for free, not a line-by-line translation.

## Reproduce

Requirements: JDK 21+ to run Gradle, a JDK 25 to run the JVM side, Python 3, and optionally Rust and Docker.

```bash
./gradlew jvmFatJar linkReleaseExecutableMacosArm64
(cd rust && cargo build --release)

python3 scripts/run.py --label my-mac \
    --java "$(/usr/libexec/java_home -v 25)/bin/java" \
    --native build/bin/macosArm64/releaseExecutable/bench.kexe \
    --rust rust/target/release/bench-rust

python3 scripts/summarize.py   # regenerates results/SUMMARY.md
```

Linux, in Docker. The Rust port is compiled inside a `rust:1.90` container. On an arm64 host:

```bash
./gradlew jvmFatJar linkReleaseExecutableLinuxArm64
scripts/run-linux-docker.sh
```

On x86-64, build `linkReleaseExecutableLinuxX64` and run `TARGET=linuxX64 scripts/run-linux-docker.sh`.

Useful options of `scripts/run.py`:

| Option | Effect |
|---|---|
| `--quick` | Ten times fewer iterations, for a smoke test |
| `--only tick noise micro:alloc-temporary` | Runs a subset of the workloads |
| `--runtimes jvm native rust` | Runs a subset of the runtimes |
| `--append` | Adds runs to an existing `results/<label>.json` |
| `--repetitions N` | Number of repetitions per workload |
| `--jvm-args="-XX:+UseZGC"` | Tries other JVM settings |

A single workload can also be run by hand:

```bash
build/bin/macosArm64/releaseExecutable/bench.kexe tick --warmup 600 --iterations 6000
java -jar build/libs/bench-jvm-all.jar tick --scale 3
rust/target/release/bench-rust micro --case alloc-temporary
```

Analysis tools:

- `scripts/profile_native_macos.py`: buckets a macOS `sample` capture of the native binary.
- `scripts/count_runtime_calls.py`: counts non-inlined runtime helper calls in each native binary.
- `experiments/blockpos-value-class.patch`: the value-class variant (apply with `patch -p1`).

The `mingwX64` (Windows) and `linuxX64` binaries build from the same sources but were not run for the published results.

## Project layout

```
src/commonMain/kotlin/bench/
  Benchmarks.kt     the workloads
  Level.kt          world, scheduled ticks, entity sections, tick phases
  Entities.kt       entities, goals, physics
  Blocks.kt         block behaviours
  Terrain.kt        chunks and terrain generation
  Network.kt        packet encoding and per-player tracking
  Noise.kt          Perlin noise (straight port and hoisted variant)
  LongObjectMap.kt  fastutil-style primitive map
  Micro.kt          micro-benchmarks, one mechanism each
  MathUtil.kt       java.util.Random clone, deterministic atan2
  Result.kt         timing and JSON output
src/jvmMain, src/nativeMain   GC counters, platform name, native GC settings
rust/               Rust port of every workload
scripts/            campaign runner, summary, Docker runner, analysis tools
experiments/        patches for variant experiments
results/            raw JSON, SUMMARY.md, experiments/ and analysis/
```
