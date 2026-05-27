from __future__ import annotations

import csv
import json
import math
import platform
import resource
import statistics
import sys
import time
from dataclasses import dataclass
from datetime import datetime, timezone
from pathlib import Path
from typing import Callable, Iterable

ROOT = Path(__file__).resolve().parents[1]
RESULTS_DIR = ROOT / "benches" / "results"
RAW_DIR = RESULTS_DIR / "raw"
SUMMARY_DIR = RESULTS_DIR / "summary"
THROUGHPUT_SIZES = [100_000, 300_000]
SEARCH_THROUGHPUT_SIZES = [100_000]
LATENCY_SAMPLES = 10_000
WARMUP_CALLS = 500
THROUGHPUT_RUNS = 5


@dataclass(frozen=True)
class Workload:
    name: str
    template: str
    factory: Callable[[int], Iterable[str]]


WORKLOADS = [
    Workload(
        name="simple_string_extraction",
        template="User {name} logged in from {ip}",
        factory=lambda n: (
            f"User user_{i} logged in from 192.168.1.{i % 256}" for i in range(n)
        ),
    ),
    Workload(
        name="mixed_type_parsing",
        template="{name:w} scored {:d} points with {:f} accuracy",
        factory=lambda n: (
            f"player_{i} scored {i * 10} points with {i * 0.01:.2f} accuracy"
            for i in range(n)
        ),
    ),
    Workload(
        name="log_style_parsing",
        template="[{level:w}] {module:w}: {message}",
        factory=lambda n: (
            f"[{'INFO' if i % 3 == 0 else 'WARN' if i % 3 == 1 else 'ERROR'}] module_{i % 10}: Event {i} occurred"
            for i in range(n)
        ),
    ),
]


SEARCH_WORKLOADS = [
    Workload(
        name="simple_string_extraction",
        template="User {name} logged in from {ip}",
        factory=lambda n: (
            f"User user_{i} logged in from 192.168.1.{i % 256}" for i in range(n)
        ),
    ),
    Workload(
        name="log_style_parsing",
        template="[{level:w}] {module:w}: {message}",
        factory=lambda n: (
            f"[{'INFO' if i % 3 == 0 else 'WARN' if i % 3 == 1 else 'ERROR'}] module_{i % 10}: Event {i} occurred"
            for i in range(n)
        ),
    ),
]


def ensure_result_dirs() -> None:
    RAW_DIR.mkdir(parents=True, exist_ok=True)
    SUMMARY_DIR.mkdir(parents=True, exist_ok=True)


def peak_rss_mb() -> float:
    usage = resource.getrusage(resource.RUSAGE_SELF).ru_maxrss
    if sys.platform == "darwin":
        return usage / (1024 * 1024)
    return usage / 1024


def percentile(values: list[float], p: float) -> float:
    if not values:
        return 0.0
    if len(values) == 1:
        return values[0]
    rank = (len(values) - 1) * p
    lower = math.floor(rank)
    upper = math.ceil(rank)
    if lower == upper:
        return values[lower]
    weight = rank - lower
    return values[lower] * (1 - weight) + values[upper] * weight


def clear_parse_rust_cache() -> None:
    import parse_rust

    clear = getattr(parse_rust, "_clear_cache", None)
    if clear is None and hasattr(parse_rust, "parse_rust"):
        clear = getattr(parse_rust.parse_rust, "_clear_cache", None)
    if clear is not None:
        clear()


def _load_parse_rust_for_fallback():
    import parse_rust

    required = [name for name in ("compile", "with_pattern") if not hasattr(parse_rust, name)]
    if not required:
        return parse_rust

    module_path = getattr(parse_rust, "__file__", "<unknown>")
    missing = ", ".join(required)
    raise RuntimeError(
        "fallback benchmark imported an incompatible parse_rust package from "
        f"{module_path}; missing exports: {missing}. "
        "Run this benchmark against the current repo build, for example by running "
        "`maturin develop` first or using the project interpreter directly."
    )


def measure_throughput(label: str, runner, iterable_factory, count: int, runs: int = 1) -> dict:
    """Measure throughput over *runs* independent trials.

    Returns mean values + ``_std`` companions when runs > 1.
    """
    times = []
    tputs = []
    rss_values = []

    for _ in range(runs):
        clear_parse_rust_cache()
        start_rss = peak_rss_mb()
        start = time.perf_counter()
        runner(iterable_factory(count))
        elapsed = time.perf_counter() - start
        end_rss = peak_rss_mb()
        times.append(elapsed)
        tputs.append(count / elapsed)
        rss_values.append(max(start_rss, end_rss))

    result = {
        "mode": label,
        "count": count,
        "runs": runs,
        "total_time_s": statistics.mean(times),
        "throughput_lines_per_s": statistics.mean(tputs),
        "peak_rss_mb": max(rss_values),
    }
    if runs > 1:
        result["total_time_s_std"] = statistics.stdev(times)
        result["throughput_lines_per_s_std"] = statistics.stdev(tputs)
    return result


def measure_latency(label: str, call_factory, sample_input: str, warm_cache: bool) -> dict:
    clear_parse_rust_cache()
    call = call_factory()
    if warm_cache:
        for _ in range(WARMUP_CALLS):
            call(sample_input)
    samples = []
    for _ in range(LATENCY_SAMPLES):
        start = time.perf_counter_ns()
        call(sample_input)
        samples.append((time.perf_counter_ns() - start) / 1_000)
    samples.sort()
    return {
        "mode": label,
        "latency_p50_us": percentile(samples, 0.50),
        "latency_p95_us": percentile(samples, 0.95),
        "latency_p99_us": percentile(samples, 0.99),
        "latency_mean_us": statistics.mean(samples),
    }


def python_parse_runner(template: str):
    from parse import parse as py_parse

    def run(lines):
        for line in lines:
            py_parse(template, line)

    return run


def python_compiled_runner(template: str):
    from parse import compile as py_compile

    parser = py_compile(template)

    def run(lines):
        for line in lines:
            parser.parse(line)

    return run


def python_search_runner(template: str):
    from parse import search as py_search

    def run(lines):
        for line in lines:
            py_search(template, line)

    return run


def python_compiled_search_runner(template: str):
    from parse import compile as py_compile

    parser = py_compile(template)

    def run(lines):
        for line in lines:
            parser.search(line)

    return run


def python_findall_runner(template: str):
    from parse import findall as py_findall

    def run(lines):
        for line in lines:
            list(py_findall(template, line))

    return run


def python_compiled_findall_runner(template: str):
    from parse import compile as py_compile

    parser = py_compile(template)

    def run(lines):
        for line in lines:
            list(parser.findall(line))

    return run


def rust_compiled_parse_runner(template: str):
    from parse_rust import compile as rs_compile

    parser = rs_compile(template)

    def run(lines):
        for line in lines:
            parser.parse(line)

    return run


def rust_parse_runner(template: str, cache_state: str):
    from parse_rust import parse as rs_parse

    def run(lines):
        warm_line = None
        for line in lines:
            if warm_line is None:
                warm_line = line
                if cache_state == "warm":
                    rs_parse(template, warm_line)
            rs_parse(template, line)

    return run


def rust_search_runner(template: str, cache_state: str):
    from parse_rust import search as rs_search

    def run(lines):
        warm_line = None
        for line in lines:
            if warm_line is None:
                warm_line = line
                if cache_state == "warm":
                    rs_search(template, warm_line)
            rs_search(template, line)

    return run


def rust_search_match_runner(template: str, cache_state: str):
    from parse_rust import search as rs_search

    def run(lines):
        warm_line = None
        for line in lines:
            if warm_line is None:
                warm_line = line
                if cache_state == "warm":
                    rs_search(template, warm_line, 0, None, None, False)
            rs_search(template, line, 0, None, None, False)

    return run


def rust_findall_runner(template: str, cache_state: str):
    from parse_rust import findall as rs_findall

    def run(lines):
        warm_line = None
        for line in lines:
            if warm_line is None:
                warm_line = line
                if cache_state == "warm":
                    list(rs_findall(template, warm_line))
            list(rs_findall(template, line))

    return run


def rust_compiled_search_match_runner(template: str):
    from parse_rust import compile as rs_compile

    parser = rs_compile(template)

    def run(lines):
        for line in lines:
            parser.search(line)

    return run


def rust_compiled_search_runner(template: str):
    from parse_rust import compile as rs_compile

    parser = rs_compile(template)

    def run(lines):
        for line in lines:
            parser.search(line)

    return run


def rust_compiled_findall_runner(template: str):
    from parse_rust import compile as rs_compile

    parser = rs_compile(template)

    def run(lines):
        for line in lines:
            list(parser.findall(line))

    return run


def python_fallback_runner():
    from parse import compile as py_compile, with_pattern as py_with_pattern

    @py_with_pattern(r"[ab]")
    def ab(text):
        return {"a": 1, "b": 2}[text]

    parser = py_compile("test {result:ab}", extra_types={"ab": ab})

    def run(lines):
        for line in lines:
            parser.parse(line)

    return run


def rust_fallback_runner():
    parse_rust = _load_parse_rust_for_fallback()
    compile = parse_rust.compile
    with_pattern = parse_rust.with_pattern

    @with_pattern(r"[ab]")
    def ab(text):
        return {"a": 1, "b": 2}[text]

    parser = compile("test {result:ab}", extra_types={"ab": ab})

    def run(lines):
        for line in lines:
            parser.parse(line)

    return run


def python_parse_call(template: str):
    from parse import parse as py_parse

    return lambda text: py_parse(template, text)


def python_compiled_call(template: str):
    from parse import compile as py_compile

    parser = py_compile(template)
    return lambda text: parser.parse(text)


def rust_parse_call(template: str):
    from parse_rust import parse as rs_parse

    return lambda text: rs_parse(template, text)


def rust_compiled_parse_call(template: str):
    from parse_rust import compile as rs_compile

    parser = rs_compile(template)
    return lambda text: parser.parse(text)


def write_raw_results(group: str, all_results: list[dict]) -> Path:
    timestamp = datetime.now(timezone.utc).strftime("%Y%m%dT%H%M%SZ")
    path = RAW_DIR / f"{group}-{timestamp}.json"
    payload = {
        "group": group,
        "generated_at": timestamp,
        "environment": {
            "python_version": sys.version,
            "platform": platform.platform(),
        },
        "results": all_results,
    }
    path.write_text(json.dumps(payload, indent=2, ensure_ascii=False), encoding="utf-8")
    return path


def _fmt_val(row: dict, key: str, fmt: str = ".6f", suffix: str = "") -> str:
    """Format a single value, with ±std suffix when *_std companion exists."""
    val = row.get(key)
    if val is None:
        return ""
    text = f"{val:{fmt}}"
    std_key = f"{key}_std"
    if std_key in row and row[std_key]:
        text += f" ± {row[std_key]:{fmt}}"
    return text + suffix


def _fmt_throughput(row: dict, key: str) -> str:
    """Format throughput as integer, with ±std when available."""
    val = row.get(key)
    if val is None:
        return ""
    text = f"{val:,.0f}"
    std_key = f"{key}_std"
    if std_key in row and row[std_key]:
        text += f" ± {row[std_key]:,.0f}"
    return text


def write_summary(group: str, all_results: list[dict]) -> tuple[Path, Path]:
    csv_path = SUMMARY_DIR / f"{group}-summary.csv"
    md_path = SUMMARY_DIR / f"{group}-summary.md"
    fieldnames = sorted({key for row in all_results for key in row.keys()})

    with csv_path.open("w", newline="", encoding="utf-8") as f:
        writer = csv.DictWriter(f, fieldnames=fieldnames)
        writer.writeheader()
        writer.writerows(all_results)

    has_std = any("_std" in k for row in all_results for k in row)
    note = ""
    if has_std:
        note = f"\nValues shown as mean ± std over {all_results[0].get('runs', 'N')} runs.\n"

    lines = [
        f"# Benchmark Summary: {group}",
        "",
        "| workload | mode | runs | count | total_time_s | throughput_lines_per_s | speedup | peak_rss_mb | latency_p50_us | latency_p95_us | latency_p99_us |",
        "|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|",
    ]
    for row in all_results:
        speedup_val = ""
        if "speedup" in row:
            speedup_val = f"{row['speedup']:.2f}x"

        lines.append(
            "| {workload} | {mode} | {runs} | {count} | {total_time_s} | {throughput} | {speedup} | {peak_rss_mb} | {p50} | {p95} | {p99} |".format(
                workload=row.get("workload", ""),
                mode=row.get("mode", ""),
                runs=str(row.get("runs", "")),
                count=row.get("count", ""),
                total_time_s=_fmt_val(row, "total_time_s", ".4f"),
                throughput=_fmt_throughput(row, "throughput_lines_per_s"),
                speedup=speedup_val,
                peak_rss_mb=f"{row['peak_rss_mb']:.2f}" if "peak_rss_mb" in row else "",
                p50=_fmt_val(row, "latency_p50_us", ".2f") if "latency_p50_us" in row else "",
                p95=_fmt_val(row, "latency_p95_us", ".2f") if "latency_p95_us" in row else "",
                p99=_fmt_val(row, "latency_p99_us", ".2f") if "latency_p99_us" in row else "",
            )
        )
    lines.append("")

    md_path.write_text("\n".join(lines) + note + "\n", encoding="utf-8")
    return csv_path, md_path


def print_console_summary(group: str, all_results: list[dict], raw_path: Path, csv_path: Path, md_path: Path) -> None:
    print(f"parse-rust benchmark group: {group}")
    print(f"Python version: {sys.version}")
    print(f"Platform: {platform.platform()}")
    print(f"Raw results: {raw_path}")
    print(f"CSV summary: {csv_path}")
    print(f"Markdown summary: {md_path}")
    print()
    for row in all_results[:10]:
        print(row)


def finalize_group(group: str, all_results: list[dict]) -> tuple[Path, Path, Path]:
    ensure_result_dirs()
    raw_path = write_raw_results(group, all_results)
    csv_path, md_path = write_summary(group, all_results)
    print_console_summary(group, all_results, raw_path, csv_path, md_path)
    return raw_path, csv_path, md_path
