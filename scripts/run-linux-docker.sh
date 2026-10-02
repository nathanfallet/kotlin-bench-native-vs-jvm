#!/usr/bin/env bash
# Runs the benchmark inside a Linux container. Build the jar and the Linux binary on the host first:
#   ./gradlew jvmFatJar linkReleaseExecutableLinuxArm64
# On an x86_64 host, pass TARGET=linuxX64.
# The Rust port is compiled for Linux inside a rust:1.90 container; set RUST=0 to skip it.
# Extra arguments are passed to scripts/run.py (e.g. --repetitions 1 --only tick).
set -euo pipefail
cd "$(dirname "$0")/.."

TARGET="${TARGET:-linuxArm64}"
LABEL="${LABEL:-linux-$(uname -m)-docker}"
RUST="${RUST:-1}"

rust_args=()
if [ "$RUST" = "1" ]; then
    docker run --rm -v "$PWD/rust":/src -w /src rust:1.90-bookworm \
        cargo build --release --target-dir target/linux
    rust_args=(--rust rust/target/linux/release/bench-rust)
fi

docker build -t kotlin-bench-native-vs-jvm docker
docker run --rm -v "$PWD":/bench -w /bench kotlin-bench-native-vs-jvm \
    python3 scripts/run.py \
    --label "$LABEL" \
    --java java \
    --native "build/bin/$TARGET/releaseExecutable/bench.kexe" \
    ${rust_args[@]+"${rust_args[@]}"} \
    "$@"
