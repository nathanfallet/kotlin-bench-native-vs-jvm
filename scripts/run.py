#!/usr/bin/env python3
"""Runs every workload on the JVM jar and the native binary, one process per run, and records the results.

Each run is wrapped in `/usr/bin/time` to capture the peak resident set size of the whole process.
JVM and native runs are interleaved so that thermal or background drift affects both targets alike.
"""
import argparse
import json
import platform
import re
import subprocess
import sys
import time
from pathlib import Path

WORKLOADS = [
    # name, arguments, default warm-up, default measured iterations
    ("tick", ["tick"], 600, 6000),
    ("tick-3x-entities", ["tick", "--scale", "3"], 300, 3000),
    ("noise", ["noise"], 30, 300),
    ("noise-hoisted", ["noise-hoisted"], 30, 300),
    ("collections", ["collections"], 30, 300),
    ("worldgen-1-thread", ["worldgen", "--threads", "1"], 5, 40),
    ("worldgen-all-threads", ["worldgen"], 5, 40),
] + [
    (f"micro:{case}", ["micro", "--case", case], 50, 300)
    for case in [
        "alloc-temporary", "alloc-retained", "value-class-temporary",
        "virtual-call-monomorphic", "virtual-call-megamorphic",
        "companion-access-in-call", "field-access-in-call",
        "list-iterator", "list-indexed", "lambda-pipeline",
        "boxed-hashset-int", "boxed-long-hashmap", "primitive-long-map",
        "array-arithmetic",
    ]
] + [
    # Rust-only variants that force heap allocation, to separate the allocator from the language.
    (f"micro:{case}", ["micro", "--case", case], 50, 300)
    for case in ["alloc-temporary-boxed", "alloc-retained-boxed"]
]

RUST_ONLY = {"micro:alloc-temporary-boxed", "micro:alloc-retained-boxed"}


def time_wrapper():
    if platform.system() == "Darwin":
        return ["/usr/bin/time", "-l"]
    return ["/usr/bin/time", "-v"]


def peak_rss_bytes(stderr: str):
    darwin = re.search(r"(\d+)\s+maximum resident set size", stderr)
    if darwin:
        return int(darwin.group(1))
    gnu = re.search(r"Maximum resident set size \(kbytes\): (\d+)", stderr)
    if gnu:
        return int(gnu.group(1)) * 1024
    return None


def run_once(command, timeout):
    started = time.perf_counter()
    process = subprocess.run(time_wrapper() + command, capture_output=True, text=True, timeout=timeout)
    wall = time.perf_counter() - started
    if process.returncode != 0:
        raise RuntimeError(f"{' '.join(command)} failed:\n{process.stdout}\n{process.stderr}")
    line = next(l for l in process.stdout.splitlines() if l.startswith("RESULT "))
    result = json.loads(line[len("RESULT "):])
    result["peakRssBytes"] = peak_rss_bytes(process.stderr)
    result["wallSeconds"] = wall
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--label", required=True, help="name of this machine/OS, used for the output file")
    parser.add_argument("--java", default="java")
    parser.add_argument("--jvm-args", default="", help="extra JVM flags, space separated")
    parser.add_argument("--jar", default="build/libs/bench-jvm-all.jar")
    parser.add_argument("--native", required=True, help="path to the native release executable")
    parser.add_argument("--rust", help="optional path to the Rust port (rust/target/release/bench-rust)")
    parser.add_argument("--repetitions", type=int, default=3)
    parser.add_argument("--startup-runs", type=int, default=20)
    parser.add_argument("--quick", action="store_true", help="10x fewer iterations, for a smoke test")
    parser.add_argument("--only", nargs="*", help="subset of workload names")
    parser.add_argument("--out", default="results")
    parser.add_argument("--runtimes", nargs="*", help="subset of runtimes to run: jvm native rust")
    parser.add_argument("--append", action="store_true",
                        help="add the new runs to an existing results/<label>.json instead of replacing it")
    args = parser.parse_args()

    targets = {
        "jvm": [args.java, *args.jvm_args.split(), "-jar", args.jar],
        "native": [args.native],
    }
    if args.rust:
        targets["rust"] = [args.rust]
    if args.runtimes:
        targets = {k: v for k, v in targets.items() if k in args.runtimes}
    java_version = subprocess.run([args.java, "-version"], capture_output=True, text=True).stderr.splitlines()[0]
    report = {
        "label": args.label,
        "machine": platform.platform(),
        "processor": platform.processor() or platform.machine(),
        "java": java_version,
        "jvmArgs": args.jvm_args,
        "date": time.strftime("%Y-%m-%d"),
        "runs": [],
        "startup": {},
    }

    for target, command in targets.items():
        samples = []
        for _ in range(args.startup_runs):
            samples.append(run_once(command + ["startup"], timeout=60))
        walls = sorted(s["wallSeconds"] for s in samples)
        rss = sorted(s["peakRssBytes"] for s in samples if s["peakRssBytes"])
        report["startup"][target] = {
            "medianWallSeconds": walls[len(walls) // 2],
            "medianPeakRssBytes": rss[len(rss) // 2] if rss else None,
        }
        print(f"startup {target:6} {walls[len(walls) // 2] * 1000:8.1f} ms", flush=True)

    for name, workload_args, warmup, iterations in WORKLOADS:
        if args.only and name not in args.only:
            continue
        if args.quick:
            warmup, iterations = max(1, warmup // 10), max(5, iterations // 10)
        extra = ["--warmup", str(warmup), "--iterations", str(iterations)]
        for repetition in range(args.repetitions):
            for target, command in targets.items():
                if name in RUST_ONLY and target != "rust":
                    continue
                result = run_once(command + workload_args + extra, timeout=3600)
                result.update({"name": name, "runtime": target, "repetition": repetition})
                report["runs"].append(result)
                rss = result["peakRssBytes"] or 0
                print(
                    f"{name:22} {target:6} rep {repetition}  mean {result['meanNs'] / 1e6:9.3f} ms"
                    f"  p99 {result['p99Ns'] / 1e6:9.3f} ms  rss {rss / 2**20:7.0f} MiB"
                    f"  gc {result['gcCount']:>5}  checksum {result['checksum']}",
                    flush=True,
                )

    out = Path(args.out)
    out.mkdir(parents=True, exist_ok=True)
    path = out / f"{args.label}.json"
    if args.append and path.exists():
        previous = json.loads(path.read_text())
        previous["runs"] += report["runs"]
        previous["startup"].update(report["startup"])
        report = previous
    path.write_text(json.dumps(report, indent=2))
    print(f"wrote {path}")


if __name__ == "__main__":
    sys.exit(main())
