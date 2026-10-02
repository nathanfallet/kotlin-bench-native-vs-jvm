#!/usr/bin/env python3
"""Turns results/*.json into the Markdown tables of results/SUMMARY.md.

For every workload, each metric is the median over the repetitions of that runtime.
"Native / JVM" is the ratio of mean durations: above 1 means native is slower.
"""
import json
import statistics
import sys
from pathlib import Path


def median(values):
    values = [v for v in values if v is not None]
    return statistics.median(values) if values else None


def ms(ns):
    return f"{ns / 1e6:.2f}"


def mib(value):
    return "n/a" if value is None else f"{value / 2**20:.0f}"


def summarize(report):
    lines = [
        f"## {report['label']}",
        "",
        f"- Machine: `{report['machine']}` ({report['processor']})",
        f"- JVM: `{report['java']}`{' with `' + report['jvmArgs'] + '`' if report['jvmArgs'] else ''}",
        f"- Date: {report['date']}",
        "",
        "### Start-up (process launch to exit, median of the start-up runs)",
        "",
        "| Runtime | Wall time (ms) | Peak RSS (MiB) |",
        "|---|---:|---:|",
    ]
    for runtime, startup in report["startup"].items():
        lines.append(f"| {runtime} | {startup['medianWallSeconds'] * 1000:.1f} | {mib(startup['medianPeakRssBytes'])} |")
    lines += [
        "",
        "### Workloads (median over repetitions)",
        "",
        "| Workload | Runtime | Mean (ms) | p50 (ms) | p99 (ms) | Max (ms) | First 50 (ms) | GCs | GC time (ms) | GC pauses (ms) | Peak RSS (MiB) | ÷ JVM mean |",
        "|---|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|",
    ]
    names = list(dict.fromkeys(run["name"] for run in report["runs"] if not run["name"].startswith("micro:")))
    checksums_ok = True
    for name in names:
        runs = [r for r in report["runs"] if r["name"] == name]
        if len({r["checksum"] for r in runs}) != 1:
            checksums_ok = False
        means = {}
        for runtime in ("jvm", "native", "rust"):
            subset = [r for r in runs if r["runtime"] == runtime]
            if not subset:
                continue
            means[runtime] = median([r["meanNs"] for r in subset])
            ratio = ""
            if runtime != "jvm" and "jvm" in means:
                ratio = f"**{means[runtime] / means['jvm']:.2f}×**"
            lines.append(
                "| " + " | ".join([
                    name if runtime == "jvm" else "",
                    runtime,
                    ms(means[runtime]),
                    ms(median([r["p50Ns"] for r in subset])),
                    ms(median([r["p99Ns"] for r in subset])),
                    ms(median([r["maxNs"] for r in subset])),
                    ms(median([r["first50MeanNs"] for r in subset])),
                    "–" if runtime == "rust" else f"{median([r['gcCount'] for r in subset]):.0f}",
                    "–" if runtime == "rust" else f"{median([r.get('gcMillis', -1) for r in subset]):.0f}",
                    "–" if runtime == "rust" else f"{median([r.get('gcPauseMillis', -1) for r in subset]):.0f}",
                    mib(median([r["peakRssBytes"] for r in subset])),
                    ratio,
                ]) + " |"
            )
    for micro in dict.fromkeys(r["name"] for r in report["runs"] if r["name"].startswith("micro:")):
        if len({r["checksum"] for r in report["runs"] if r["name"] == micro}) != 1:
            checksums_ok = False
    lines += phase_table(report) + micro_table(report)
    lines += [
        "",
        "Checksums: " + ("identical across runtimes and repetitions for every workload."
                         if checksums_ok else "**MISMATCH** — the runtimes did not compute the same result."),
        "",
    ]
    return "\n".join(lines)


def phase_table(report):
    lines = []
    for name in ("tick", "tick-3x-entities"):
        runs = [r for r in report["runs"] if r["name"] == name]
        if not runs:
            continue
        phases = [k for k in runs[0] if k.startswith("phase.")]
        lines += [
            "",
            f"### `{name}` per phase (mean ms per tick, median over repetitions)",
            "",
        ]
        has_rust = any(r["runtime"] == "rust" for r in runs)
        lines.append("| Phase | JVM | Native | Native / JVM |" + (" Rust | Rust / JVM |" if has_rust else ""))
        lines.append("|---|---:|---:|---:|" + ("---:|---:|" if has_rust else ""))
        for key in phases:
            values = {}
            for runtime in ("jvm", "native", "rust"):
                values[runtime] = median([int(r[key]) for r in runs if r["runtime"] == runtime and key in r])
            def ratio(v):
                return f"{v / values['jvm']:.2f}×" if values["jvm"] and v is not None else "n/a"
            label = key.split(".")[1]
            row = f"| {label} | {ms(values['jvm'])} | {ms(values['native'])} | {ratio(values['native'])} |"
            if has_rust:
                row += f" {ms(values['rust']) if values['rust'] is not None else 'n/a'} | {ratio(values['rust'])} |"
            lines.append(row)
    return lines


def micro_table(report):
    runs = [r for r in report["runs"] if r["name"].startswith("micro:")]
    if not runs:
        return []
    lines = [
        "",
        "### Micro-benchmarks (nanoseconds per operation, median over repetitions)",
        "",
    ]
    has_rust = any(r["runtime"] == "rust" for r in runs)
    header = "| Mechanism | What it does | JVM (ns/op) | Native (ns/op) | Native / JVM |"
    rule = "|---|---|---:|---:|---:|"
    if has_rust:
        header += " Rust (ns/op) | Rust / JVM |"
        rule += "---:|---:|"
    lines += [header, rule]
    for name in dict.fromkeys(r["name"] for r in runs):
        subset = [r for r in runs if r["name"] == name]
        kotlin = [r for r in subset if r["runtime"] != "rust"]
        operations = int(subset[0]["operations"])
        values = {rt: median([r["meanNs"] for r in subset if r["runtime"] == rt]) for rt in ("jvm", "native", "rust")}
        values = {rt: (v / operations if v is not None else None) for rt, v in values.items()}
        row = f"| `{name.split(':', 1)[1]}` | {(kotlin or subset)[0]['description']} | "
        if values["jvm"] is not None:
            row += f"{values['jvm']:.2f} | {values['native']:.2f} | **{values['native'] / values['jvm']:.2f}×** |"
        else:
            row += " | | |"
        if has_rust:
            if values["rust"] is None:
                row += " | |"
            elif values["jvm"] is not None:
                row += f" {values['rust']:.2f} | {values['rust'] / values['jvm']:.2f}× |"
            else:
                row += f" {values['rust']:.2f} | |"
        lines.append(row)
    return lines


def main():
    directory = Path(sys.argv[1] if len(sys.argv) > 1 else "results")
    reports = [json.loads(p.read_text()) for p in sorted(directory.glob("*.json"))]
    output = "# Results\n\nGenerated by `scripts/summarize.py` from the JSON files in this directory.\n\n"
    output += "\n".join(summarize(r) for r in reports)
    (directory / "SUMMARY.md").write_text(output)
    print(output)


if __name__ == "__main__":
    main()
