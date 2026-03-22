"""
Performance benchmark: parse-rust vs Python parse

Compares throughput, latency, and memory usage between
the original Python parse library and the Rust rewrite.

Usage:
    python benches/benchmark.py
"""

import time
import sys

def benchmark_python_parse(template, test_lines):
    """Benchmark the original Python parse library."""
    from parse import parse as py_parse

    start = time.perf_counter()
    for line in test_lines:
        py_parse(template, line)
    elapsed = time.perf_counter() - start
    return elapsed

def benchmark_rust_parse(template, test_lines):
    """Benchmark the Rust parse_rust library."""
    from parse_rust import parse as rs_parse

    start = time.perf_counter()
    for line in test_lines:
        rs_parse(template, line)
    elapsed = time.perf_counter() - start
    return elapsed

def benchmark_rust_compiled(template, test_lines):
    """Benchmark Rust with pre-compiled parser."""
    from parse_rust import compile as rs_compile

    parser = rs_compile(template)
    start = time.perf_counter()
    for line in test_lines:
        parser.parse(line)
    elapsed = time.perf_counter() - start
    return elapsed

def benchmark_python_compiled(template, test_lines):
    """Benchmark Python with pre-compiled parser."""
    from parse import compile as py_compile

    parser = py_compile(template)
    start = time.perf_counter()
    for line in test_lines:
        parser.parse(line)
    elapsed = time.perf_counter() - start
    return elapsed

def run_benchmark(name, template, test_lines):
    """Run a full benchmark comparison."""
    print(f"\n{'='*60}")
    print(f"Benchmark: {name}")
    print(f"Lines: {len(test_lines):,}")
    print(f"{'='*60}")

    # Python parse (one-shot)
    py_time = benchmark_python_parse(template, test_lines)
    py_rate = len(test_lines) / py_time

    # Rust parse (one-shot)
    rs_time = benchmark_rust_parse(template, test_lines)
    rs_rate = len(test_lines) / rs_time

    # Python compiled
    py_comp_time = benchmark_python_compiled(template, test_lines)
    py_comp_rate = len(test_lines) / py_comp_time

    # Rust compiled
    rs_comp_time = benchmark_rust_compiled(template, test_lines)
    rs_comp_rate = len(test_lines) / rs_comp_time

    print(f"\n{'Method':<25} {'Time (s)':<12} {'Rate (lines/s)':<18} {'Speedup':<10}")
    print(f"{'-'*65}")
    print(f"{'Python (one-shot)':<25} {py_time:<12.3f} {py_rate:<18,.0f} {'1.0x':<10}")
    print(f"{'Rust (one-shot)':<25} {rs_time:<12.3f} {rs_rate:<18,.0f} {f'{py_time/rs_time:.1f}x':<10}")
    print(f"{'Python (compiled)':<25} {py_comp_time:<12.3f} {py_comp_rate:<18,.0f} {f'{py_time/py_comp_time:.1f}x':<10}")
    print(f"{'Rust (compiled)':<25} {rs_comp_time:<12.3f} {rs_comp_rate:<18,.0f} {f'{py_time/rs_comp_time:.1f}x':<10}")

    return {
        "name": name,
        "python_time": py_time,
        "rust_time": rs_time,
        "python_compiled_time": py_comp_time,
        "rust_compiled_time": rs_comp_time,
        "speedup_oneshot": py_time / rs_time,
        "speedup_compiled": py_comp_time / rs_comp_time,
    }

def main():
    N = 100_000  # Number of test lines

    print("parse-rust Performance Benchmark")
    print(f"Python version: {sys.version}")

    # Benchmark 1: Simple string extraction
    template1 = "User {name} logged in from {ip}"
    lines1 = [
        f"User user_{i} logged in from 192.168.1.{i % 256}"
        for i in range(N)
    ]
    r1 = run_benchmark("Simple string extraction", template1, lines1)

    # Benchmark 2: Mixed types
    template2 = "{name:w} scored {:d} points with {:f} accuracy"
    lines2 = [
        f"player_{i} scored {i * 10} points with {i * 0.01:.2f} accuracy"
        for i in range(N)
    ]
    r2 = run_benchmark("Mixed type parsing", template2, lines2)

    # Benchmark 3: Log-style parsing
    template3 = "[{level:w}] {module:w}: {message}"
    lines3 = [
        f"[{'INFO' if i % 3 == 0 else 'WARN' if i % 3 == 1 else 'ERROR'}] module_{i % 10}: Event {i} occurred"
        for i in range(N)
    ]
    r3 = run_benchmark("Log-style parsing", template3, lines3)

    # Summary
    print(f"\n{'='*60}")
    print("SUMMARY")
    print(f"{'='*60}")
    avg_speedup_oneshot = sum(r["speedup_oneshot"] for r in [r1, r2, r3]) / 3
    avg_speedup_compiled = sum(r["speedup_compiled"] for r in [r1, r2, r3]) / 3
    print(f"Average speedup (one-shot): {avg_speedup_oneshot:.1f}x")
    print(f"Average speedup (compiled): {avg_speedup_compiled:.1f}x")

if __name__ == "__main__":
    main()
