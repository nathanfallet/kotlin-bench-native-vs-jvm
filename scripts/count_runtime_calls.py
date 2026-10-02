#!/usr/bin/env python3
"""Counts call sites to Kotlin/Native runtime helpers in each native release binary.

A call site means the helper was NOT inlined into user code. On macosArm64 and linuxX64, Kotlin/Native
inlines most of them (array access, frame bookkeeping, type checks); this script shows whether a target does.

Usage: python3 scripts/count_runtime_calls.py [path/to/llvm-objdump]
Needs llvm-objdump (Xcode's, Homebrew's llvm, or the one shipped in ~/.konan/dependencies).
"""
import re
import subprocess
import sys
from collections import Counter
from pathlib import Path

TARGETS = ["macosArm64", "linuxArm64", "linuxX64"]
HELPER = re.compile(
    r"\b(?:bl|callq?)\s+(?:0x[0-9a-f]+\s+)?<_?("
    r"(?:Kotlin_|EnterFrame|LeaveFrame|AllocInstance|AllocArrayInstance|CallInitGlobalPossiblyLock)[^>+]*)"
)


def main():
    objdump = sys.argv[1] if len(sys.argv) > 1 else "llvm-objdump"
    counts = {}
    for target in TARGETS:
        binary = Path(f"build/bin/{target}/releaseExecutable/bench.kexe")
        if not binary.exists():
            continue
        disassembly = subprocess.run([objdump, "-d", "--no-show-raw-insn", str(binary)],
                                     capture_output=True, text=True, check=True).stdout
        counter = Counter()
        for match in HELPER.finditer(disassembly):
            # Fold IntArray_get, DoubleArray_get... into one line per operation.
            counter[re.sub(r"(Int|Double|Long|Byte|Short|Char|Float|Boolean)Array", "XArray", match.group(1))] += 1
        counts[target] = counter

    targets = list(counts)
    helpers = sorted({h for c in counts.values() for h, _ in c.most_common(20)},
                     key=lambda h: -max(c[h] for c in counts.values()))
    print("| Runtime helper | " + " | ".join(targets) + " |")
    print("|---|" + "---:|" * len(targets))
    for helper in helpers[:20]:
        print(f"| `{helper}` | " + " | ".join(str(counts[t][helper]) for t in targets) + " |")


if __name__ == "__main__":
    main()
