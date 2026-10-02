#!/usr/bin/env python3
"""Summarises a macOS `sample` capture of the native binary into self-time buckets per thread.

Usage:
    bench.kexe tick --warmup 0 --iterations 5000 &
    sleep 25; sample <pid> 15 -file native-sample.txt
    python3 scripts/profile_native_macos.py native-sample.txt

Each frame's self time is its sample count minus the counts of its children. Frames are then grouped
into buckets (allocation, GC, thread-local access, collections, game code...) by symbol name.
"""
import re
import sys
from collections import Counter, defaultdict

LINE = re.compile(r"^(?P<prefix>[\s+!:|]*?)(?P<count>\d+)\s+(?P<symbol>.+?)(\s+\(in (?P<image>[^)]+)\))?(\s+\+ \d+)?(\s+\[0x[0-9a-f]+(,0x[0-9a-f]+)*\])?\s*$")
THREAD = re.compile(r"^\s+(\d+)\s+Thread_\d+(:\s*(?P<name>.*))?")

BUCKETS = [
    ("idle / waiting", r"__psynch_cvwait|__semwait|__workq_kernreturn|mach_msg|__ulock_wait|nanosleep|_pthread_cond_wait"),
    ("GC: mark", r"gc::Mark|processFieldInMark|processObjectInMark|processArrayInMark|MarkQueue|ConcurrentMark"),
    ("GC: sweep", r"Sweep|Finalizer"),
    ("allocation", r"CustomAllocator|alloc::|Allocate|_platform_memset|AllocInstance|AllocArray"),
    ("thread-local access (_tlv_get_addr)", r"_tlv_get_addr"),
    ("lazy / global init checks", r"SynchronizedLazyImpl|CallInitGlobal|InitSharedInstance|CallInitThreadLocal"),
    ("kotlin.collections (HashMap, ArrayList, iterators)", r"kotlin\.collections|kotlin\.sequences"),
    ("boxing and equals bridges", r"bridge-|Int#equals|Long#equals|boxCache|Int#hashCode|box"),
    ("game code (bench.*)", r"kfun:bench\."),
]


def bucket_of(symbol: str) -> str:
    for name, pattern in BUCKETS:
        if re.search(pattern, symbol):
            return name
    return "other runtime / libc"


def main(path: str) -> None:
    threads = defaultdict(Counter)
    thread_name = None
    stack = []  # (depth, count, symbol)
    in_tree = False

    def flush_until(depth):
        while stack and stack[-1][0] >= depth:
            d, count, symbol, children = stack.pop()
            self_time = count - children
            if self_time > 0 and thread_name:
                threads[thread_name][symbol] += self_time
            if stack:
                stack[-1][3] += count

    for raw in open(path, errors="replace"):
        if raw.startswith("Call graph:"):
            in_tree = True
            continue
        if in_tree and (raw.startswith("Total number in stack") or raw.startswith("Sort by top of stack")):
            flush_until(-1)
            break
        if not in_tree:
            continue
        thread = THREAD.match(raw)
        if thread:
            flush_until(-1)
            thread_name = (thread.group("name") or "unnamed").strip() or "unnamed"
            continue
        match = LINE.match(raw.rstrip("\n"))
        if not match or thread_name is None:
            continue
        depth = len(match.group("prefix"))
        flush_until(depth)
        stack.append([depth, int(match.group("count")), match.group("symbol").strip(), 0])

    for name, frames in threads.items():
        total = sum(frames.values())
        busy = total - sum(c for s, c in frames.items() if bucket_of(s) == "idle / waiting")
        if busy <= total * 0.05:
            continue
        buckets = Counter()
        for symbol, count in frames.items():
            buckets[bucket_of(symbol)] += count
        print(f"\n## Thread: {name}  ({total} samples, {busy / total:.0%} busy)\n")
        print("| Bucket | Share of busy time |")
        print("|---|---:|")
        for bucket, count in buckets.most_common():
            if bucket != "idle / waiting":
                print(f"| {bucket} | {count / busy:.1%} |")
        print("\nTop self-time frames:\n")
        for symbol, count in frames.most_common(15):
            if bucket_of(symbol) != "idle / waiting":
                print(f"- {count / busy:5.1%}  {symbol[:110]}")


if __name__ == "__main__":
    main(sys.argv[1])
