import os
import platform
import re
import statistics
import subprocess
import sys
from dataclasses import dataclass
from typing import List, Optional, Tuple


LINE_RE = re.compile(
    r"^lang=(?P<lang>[a-zA-Z0-9_+-]+)\s+iters=(?P<iters>\d+)\s+total_ns=(?P<total>\d+)\s+per_ns=(?P<per>\d+)\s*$"
)


@dataclass(frozen=True)
class Sample:
    lang: str
    iters: int
    total_ns: int
    per_ns: int


def sh(cmd: List[str], cwd: Optional[str] = None) -> str:
    p = subprocess.run(
        cmd,
        cwd=cwd,
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
        text=True,
        check=True,
    )
    return p.stdout


def parse_sample(output: str) -> Sample:
    lines = [ln.strip() for ln in output.splitlines() if ln.strip()]
    last = lines[-1] if lines else ""
    m = LINE_RE.match(last)
    if not m:
        raise RuntimeError(f"unexpected output, last line={last!r}\nfull:\n{output}")
    return Sample(
        lang=m.group("lang"),
        iters=int(m.group("iters")),
        total_ns=int(m.group("total")),
        per_ns=int(m.group("per")),
    )


def run_many(cmd: List[str], rounds: int, cwd: str) -> List[Sample]:
    out: List[Sample] = []
    for _ in range(rounds):
        s = parse_sample(sh(cmd, cwd=cwd))
        out.append(s)
    return out


def pct(x: float) -> str:
    return f"{x * 100:.1f}%"


def ns_to_ms(ns: int) -> float:
    return ns / 1_000_000.0


def summarize(samples: List[Sample]) -> Tuple[Sample, float, float]:
    per = [s.per_ns for s in samples]
    med = statistics.median(per)
    mean = statistics.mean(per)
    stdev = statistics.pstdev(per) if len(per) > 1 else 0.0
    exemplar = samples[0]
    return exemplar, med, stdev


def format_row(lang: str, iters: int, median_per_ns: float, stdev_ns: float, vs_python: Optional[float]) -> str:
    median_us = median_per_ns / 1000.0
    stdev_us = stdev_ns / 1000.0
    ratio = "" if vs_python is None else f"{vs_python:.2f}x"
    return f"| {lang} | {iters} | {median_us:,.3f} µs/op | {stdev_us:,.3f} µs | {ratio} |"


def main() -> int:
    repo = os.path.abspath(os.path.join(os.path.dirname(__file__), ".."))
    rounds = int(os.environ.get("ROUNDS", "7"))

    sh(["cargo", "build", "-q", "-p", "xu_cli"], cwd=repo)
    xu_bin = os.path.join(repo, "target", "debug", "xu")
    xu_cmd = [xu_bin, "run", "lang=en", "bench/dict_merge.xu"]
    py_cmd = ["python3", "bench/dict_merge.py"]
    node_cmd = ["node", "bench/dict_merge.js"]

    py_ver = sys.version.splitlines()[0]
    node_ver = sh(["node", "--version"], cwd=repo).strip()
    rustc_ver = sh(["rustc", "--version"], cwd=repo).strip()
    cargo_ver = sh(["cargo", "--version"], cwd=repo).strip()
    os_ver = platform.platform()

    cpu = ""
    try:
        cpu = sh(["sysctl", "-n", "machdep.cpu.brand_string"], cwd=repo).strip()
    except Exception:
        cpu = platform.processor()

    samples_py = run_many(py_cmd, rounds=rounds, cwd=repo)
    samples_node = run_many(node_cmd, rounds=rounds, cwd=repo)
    samples_xu = run_many(xu_cmd, rounds=rounds, cwd=repo)

    py_ex, py_median, py_sd = summarize(samples_py)
    node_ex, node_median, node_sd = summarize(samples_node)
    xu_ex, xu_median, xu_sd = summarize(samples_xu)

    rows = []
    rows.append(format_row("python", py_ex.iters, py_median, py_sd, None))
    rows.append(format_row("node", node_ex.iters, node_median, node_sd, node_median / py_median))
    rows.append(format_row("xu", xu_ex.iters, xu_median, xu_sd, xu_median / py_median))

    print("# Dict merge micro-benchmark\n")
    print("## Environment")
    print(f"- OS: {os_ver}")
    if cpu:
        print(f"- CPU: {cpu}")
    print(f"- Python: {py_ver}")
    print(f"- Node: {node_ver}")
    print(f"- Rust: {rustc_ver}")
    print(f"- Cargo: {cargo_ver}")
    print(f"- Rounds: {rounds}")
    print("")
    print("## Workload")
    print("- Operation: in-place merge/update of a 200-key dict with a 200-key dict")
    print("- Warmup: 2,000 iterations (inside each program)")
    print("- Timed: 200,000 iterations (inside each program)")
    print("")
    print("## Results (median per-op, based on internal monotonic clock)")
    print("| Lang | Iters | Median | σ | vs Python |")
    print("| --- | ---: | ---: | ---: | ---: |")
    for r in rows:
        print(r)
    print("")
    print("## Notes")
    print("- This is a micro-benchmark; absolute numbers vary with CPU scaling and background load.")
    print("- Xu timing uses stdlib time.mono_nanos(); Python uses perf_counter_ns(); Node uses hrtime.bigint().")

    return 0


if __name__ == "__main__":
    raise SystemExit(main())
