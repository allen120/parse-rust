"""
Benchmark entrypoint for parse-rust.

The full P4 benchmark matrix is split into smaller scripts so large runs do not
block until every scenario finishes.

Recommended usage:
    python benches/benchmark_parse_throughput.py
    python benches/benchmark_search_throughput.py
    python benches/benchmark_findall_throughput.py
    python benches/benchmark_parse_latency.py
    python benches/benchmark_fallback_throughput.py
"""

from pathlib import Path


def main() -> None:
    scripts = [
        "benchmark_parse_throughput.py",
        "benchmark_search_throughput.py",
        "benchmark_findall_throughput.py",
        "benchmark_parse_latency.py",
        "benchmark_fallback_throughput.py",
    ]
    print("parse-rust benchmark suite has been split into grouped scripts:")
    for script in scripts:
        print(f"- python benches/{script}")
    print()
    print("Each script writes its own raw JSON and summary files under benches/results/.")
    print(f"Common helpers live in: {Path(__file__).with_name('benchmark_common.py')}")


if __name__ == "__main__":
    main()
