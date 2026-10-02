//! Rust port of the Kotlin benchmark, used as a third column next to Kotlin/JVM and Kotlin/Native.
//!
//! Every workload does the same work as its Kotlin counterpart (same random stream, same iteration orders,
//! same floating-point operations), which the checksum proves. The code is otherwise written as Rust is
//! normally written: small value types are `Copy` structs, entities live in an arena and refer to each other
//! by index, behaviour is dispatched with `match` over enums. See `level.rs` and `entities.rs` for the design.

mod benchmarks;
mod blocks;
mod entities;
mod geometry;
mod level;
mod long_map;
mod micro;
mod network;
mod noise;
mod result;
mod terrain;
mod util;

use std::collections::HashMap;

const USAGE: &str = "usage: bench-rust <startup|tick|noise|noise-hoisted|collections|worldgen|micro> \
[--warmup N] [--iterations N] [--seed N] [--threads N] [--scale X] [--case NAME] [--trace true]";

pub struct Options {
    pub workload: String,
    pub warmup: Option<usize>,
    pub iterations: Option<usize>,
    pub seed: i64,
    pub threads: Option<usize>,
    pub scale: f64,
    pub case: Option<String>,
    /// Debugging aid: prints the level checksum and entity count after every tick to stderr.
    pub trace: bool,
}

impl Options {
    fn parse(args: &[String]) -> Options {
        let workload = args.first().unwrap_or_else(|| panic!("{USAGE}")).clone();
        let mut flags = HashMap::new();
        let mut i = 1;
        while i < args.len() {
            let name = args[i].trim_start_matches("--").to_string();
            let value = args.get(i + 1).unwrap_or_else(|| panic!("missing value for --{name}\n{USAGE}"));
            flags.insert(name, value.clone());
            i += 2;
        }
        let parse = |name: &str| flags.get(name).map(|v| v.parse().unwrap_or_else(|_| panic!("bad --{name}")));
        Options {
            workload,
            warmup: parse("warmup"),
            iterations: parse("iterations"),
            seed: flags.get("seed").map_or(20260930, |v| v.parse().expect("bad --seed")),
            threads: parse("threads"),
            scale: flags.get("scale").map_or(1.0, |v| v.parse().expect("bad --scale")),
            case: flags.get("case").cloned(),
            trace: flags.get("trace").is_some_and(|v| v == "true"),
        }
    }
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let options = Options::parse(&args);
    let result = match options.workload.as_str() {
        "startup" => {
            // Smallest possible run: measures process start-up only.
            println!("RESULT {{\"target\":\"{}\",\"workload\":\"startup\"}}", result::platform_name());
            return;
        }
        "tick" => benchmarks::tick(&options),
        "noise" => benchmarks::noise(&options, false),
        "noise-hoisted" => benchmarks::noise(&options, true),
        "collections" => benchmarks::collections(&options),
        "worldgen" => benchmarks::worldgen(&options),
        "micro" => {
            micro::run(&options);
            return;
        }
        other => panic!("unknown workload '{other}'\n{USAGE}"),
    };
    println!("{}", result.to_json());
}
