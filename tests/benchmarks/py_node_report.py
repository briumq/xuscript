import argparse
import json
import math
import os
import platform
import resource
import signal
import statistics
import subprocess
import sys
import time

# 防护配置
MAX_MEMORY_MB = int(os.environ.get("BENCH_MAX_MEMORY_MB", "2048"))  # 默认 2GB 内存上限
SINGLE_RUN_TIMEOUT = int(os.environ.get("BENCH_SINGLE_TIMEOUT", "600"))  # 单次运行超时 600 秒
TOTAL_TIMEOUT = int(os.environ.get("BENCH_TOTAL_TIMEOUT", "1800"))  # 总超时 30 分钟


def set_memory_limit():
    """设置内存使用上限，防止系统卡死"""
    try:
        soft, hard = resource.getrlimit(resource.RLIMIT_AS)
        limit_bytes = MAX_MEMORY_MB * 1024 * 1024
        resource.setrlimit(resource.RLIMIT_AS, (limit_bytes, hard))
        print(f"[Guard] Memory limit set to {MAX_MEMORY_MB} MB", file=sys.stderr)
    except (ValueError, resource.error) as e:
        print(f"[Guard] Warning: Could not set memory limit: {e}", file=sys.stderr)


class TimeoutError(Exception):
    pass


def timeout_handler(signum, frame):
    raise TimeoutError("Total benchmark timeout exceeded")


def ensure_xu_bin():
    subprocess.run(["cargo","build","-q","-p","xu_cli","--bin","xu","--release"], check=True)
    return os.path.abspath("target/release/xu")


CASES = None


def run(cmd, timeout=SINGLE_RUN_TIMEOUT):
    """运行命令，带超时保护"""
    try:
        p = subprocess.run(cmd, capture_output=True, text=True, timeout=timeout)
        if p.returncode != 0:
            raise RuntimeError(
                f"command failed ({p.returncode}): {' '.join(cmd)}\n{p.stdout}\n{p.stderr}"
            )
        return p.stdout
    except subprocess.TimeoutExpired:
        print(f"[Guard] Command timed out after {timeout}s: {' '.join(cmd)}", file=sys.stderr)
        raise RuntimeError(f"Command timed out: {' '.join(cmd)}")


def version_of(cmd):
    p = subprocess.run(cmd, capture_output=True, text=True)
    s = (p.stdout or p.stderr).strip()
    return s


def parse_jsonl(out):
    rows = {}
    for line in out.splitlines():
        line = line.strip()
        if not line:
            continue
        obj = json.loads(line)
        rows[obj["case"]] = float(obj["duration_ms"])
    return rows


def bench_once(scale):
    """运行一次 benchmark，带错误恢复"""
    try:
        out = run(["bash", "scripts/run_cross_lang_bench.sh", str(scale)], timeout=SINGLE_RUN_TIMEOUT)
    except RuntimeError as e:
        print(f"[Guard] bench_once failed: {e}", file=sys.stderr, flush=True)
        return ({}, {}, {}, {}, {}, {})  # 返回空结果，继续下一轮
    py = {}
    nd = {}
    xu = {}
    pym = {}
    ndm = {}
    xum = {}
    section = None
    for raw in out.splitlines():
        line = raw.strip()
        if not line:
            continue
        if line == "Python:":
            section = "py"
            continue
        if line == "Node.js:":
            section = "node"
            continue
        if line == "Xu:":
            section = "xu"
            continue
        if section == "py" and line.startswith("{"):
            obj = json.loads(line)
            py[obj["case"]] = float(obj["duration_ms"])
            if "rss_bytes" in obj:
                pym[obj["case"]] = float(obj["rss_bytes"]) / (1024.0 * 1024.0)
            continue
        if section == "node" and line.startswith("{"):
            obj = json.loads(line)
            nd[obj["case"]] = float(obj["duration_ms"])
            if "rss_bytes" in obj:
                ndm[obj["case"]] = float(obj["rss_bytes"]) / (1024.0 * 1024.0)
            continue
        if section == "xu" and line.startswith("{"):
            obj = json.loads(line)
            xu[obj["case"]] = float(obj["duration_ms"])
            if "rss_bytes" in obj:
                xum[obj["case"]] = float(obj["rss_bytes"]) / (1024.0 * 1024.0)
            continue
    # Fallback: run Xu directly to capture JSON if shell wrapper printed none
    if not xu:
        xu_bin = ensure_xu_bin()
        env = os.environ.copy()
        env["BENCH_SCALE"] = str(scale)
        env["BENCH_SMOKE"] = "0"
        try:
            p = subprocess.run([xu_bin, "run", "--no-diags", "tests/benchmarks/xu/bench.xu"],
                             capture_output=True, text=True, env=env, timeout=SINGLE_RUN_TIMEOUT)
            for line in p.stdout.splitlines():
                line = line.strip()
                if line.startswith("{"):
                    obj = json.loads(line)
                    xu[obj["case"]] = float(obj["duration_ms"])
        except subprocess.TimeoutExpired:
            print(f"[Guard] Xu fallback timed out", file=sys.stderr, flush=True)
    return (py, nd, xu, pym, ndm, xum)


def fmt_ms(x):
    if not math.isfinite(x):
        return "-"
    if x < 1.0:
        return f"{x:.2f}"
    if abs(x - round(x)) < 1e-9:
        return str(int(round(x)))
    return f"{x:.2f}"


def stats(xs):
    xs = list(xs)
    if not xs:
        return {"min": float("nan"), "median": float("nan"), "mean": float("nan"), "p95": float("nan"), "max": float("nan"), "stdev": float("nan")}
    xs_sorted = sorted(xs)
    p95_index = max(0, int(0.95 * (len(xs_sorted) - 1)))
    return {
        "min": xs_sorted[0],
        "median": statistics.median(xs_sorted),
        "mean": statistics.mean(xs_sorted),
        "p95": xs_sorted[p95_index],
        "max": xs_sorted[-1],
        "stdev": statistics.stdev(xs_sorted) if len(xs_sorted) > 1 else 0.0,
    }


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--scales", default="5000,10000")
    ap.add_argument("--runs", type=int, default=10)
    ap.add_argument("--out", default="tests/benchmarks/report.md")
    ap.add_argument("--no-memory-limit", action="store_true", help="Disable memory limit protection")
    args = ap.parse_args()

    # 设置防护
    if not args.no_memory_limit:
        set_memory_limit()

    # 设置总超时
    signal.signal(signal.SIGALRM, timeout_handler)
    signal.alarm(TOTAL_TIMEOUT)
    print(f"[Guard] Total timeout set to {TOTAL_TIMEOUT}s ({TOTAL_TIMEOUT // 60} min)", file=sys.stderr)

    try:
        _run_benchmarks(args)
    except TimeoutError as e:
        print(f"[Guard] {e}", file=sys.stderr)
        sys.exit(1)
    finally:
        signal.alarm(0)  # 取消超时


def _run_benchmarks(args):

    scales = [int(x.strip()) for x in args.scales.split(",") if x.strip()]
    if not scales:
        scales = [5000, 10000]

    python_ver = version_of([sys.executable, "--version"])
    node_ver = version_of(["node", "--version"])
    rust_ver = version_of(["rustc", "-V"])

    case_order = [
        "loop",
        "dict",
        "dict-intkey",
        "dict-hot",
        "string",
        "string-builder",
        "struct-method",
        "func-call",
        "branch-heavy",
        "list-push-pop",
        "dict-miss",
        "dict-update-hot",
        "string-unicode",
        "string-scan",
    ]

    md = []
    md.append("# Benchmarks Report (Python vs Node.js vs Xu)")
    md.append("")
    md.append(f"- Generated: {int(time.time())}")
    md.append(f"- Runs per scale: {args.runs}")
    md.append(f"- Scales: {', '.join(str(s) for s in scales)}")
    md.append(f"- OS: {platform.system().lower()} / {platform.machine().lower()}")
    if rust_ver:
        md.append(f"- Rust: {rust_ver}")
    if python_ver:
        md.append(f"- Python: {python_ver}")
    if node_ver:
        md.append(f"- Node: {node_ver}")
    md.append("")

    hist = {"scales": scales, "runs": args.runs, "generated": int(time.time()), "results": []}
    for scale in scales:
        print(f"[Progress] Starting scale {scale}...", file=sys.stderr, flush=True)
        agg_py = {}
        agg_nd = {}
        agg_xu = {}
        agg_pym = {}
        agg_ndm = {}
        agg_xum = {}
        for run_idx in range(args.runs):
            print(f"[Progress] Scale {scale}, run {run_idx + 1}/{args.runs}...", file=sys.stderr, flush=True)
            py, nd, xu, pym, ndm, xum = bench_once(scale)
            for k, v in py.items():
                agg_py.setdefault(k, []).append(v)
            for k, v in nd.items():
                agg_nd.setdefault(k, []).append(v)
            for k, v in xu.items():
                agg_xu.setdefault(k, []).append(v)
            for k, v in pym.items():
                agg_pym.setdefault(k, []).append(v)
            for k, v in ndm.items():
                agg_ndm.setdefault(k, []).append(v)
            for k, v in xum.items():
                agg_xum.setdefault(k, []).append(v)

        print(f"[Progress] Generating report for scale {scale}...", file=sys.stderr, flush=True)
        md.append(f"## Scale {scale}")
        md.append("")
        md.append("| case | Python median (ms) | Node.js median (ms) | Xu median (ms) | Python p95 | Node p95 | Xu p95 | Python op/s | Node op/s | Xu op/s | jitter | Py mem (MB) | Node mem (MB) | Xu mem (MB) | winner |")
        md.append("|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---|")
        scale_result = {"scale": scale, "cases": {}}
        for c in case_order:
            sp = stats(agg_py.get(c, []))
            sn = stats(agg_nd.get(c, []))
            sx = stats(agg_xu.get(c, []))
            vals = [("Python", sp["median"]), ("Node.js", sn["median"]), ("Xu", sx["median"])]
            finite = [(k, v) for (k, v) in vals if math.isfinite(v)]
            winner = min(finite, key=lambda x: x[1])[0] if finite else "-"
            def ops(scale, ms):
                return "-" if not math.isfinite(ms) or ms <= 0 else f"{(scale / ms * 1000.0):.0f}"
            # jitter 使用 (max - min) / median
            jitter_vals = [sp, sn, sx]
            jitter = "-"
            if all(math.isfinite(v['min']) and math.isfinite(v['max']) and math.isfinite(v['median']) and v['median'] > 0 for v in jitter_vals):
                pj = (sp['max'] - sp['min']) / sp['median'] if sp['median'] > 0 else float('nan')
                nj = (sn['max'] - sn['min']) / sn['median'] if sn['median'] > 0 else float('nan')
                xj = (sx['max'] - sx['min']) / sx['median'] if sx['median'] > 0 else float('nan')
                jitter = f"P:{pj:.2f} N:{nj:.2f} X:{xj:.2f}"
            mp = stats(agg_pym.get(c, []))["median"]
            mn = stats(agg_ndm.get(c, []))["median"]
            mx = stats(agg_xum.get(c, []))["median"]
            md.append(f"| {c} | {fmt_ms(sp['median'])} | {fmt_ms(sn['median'])} | {fmt_ms(sx['median'])} | {fmt_ms(sp['p95'])} | {fmt_ms(sn['p95'])} | {fmt_ms(sx['p95'])} | {ops(scale, sp['median'])} | {ops(scale, sn['median'])} | {ops(scale, sx['median'])} | {jitter} | {fmt_ms(mp)} | {fmt_ms(mn)} | {fmt_ms(mx)} | {winner} |")
            scale_result["cases"][c] = {
                "py": sp["median"],
                "node": sn["median"],
                "xu": sx["median"],
                "py_mem": mp if math.isfinite(mp) else None,
                "node_mem": mn if math.isfinite(mn) else None,
                "xu_mem": mx if math.isfinite(mx) else None,
            }
        hist["results"].append(scale_result)
        md.append("")

        md.append("### Notes")
        md.append(
            "- Numbers are medians over repeated runs of the same per-case microbenchmark."
        )
        md.append("")

    print(f"[Progress] Writing report to {args.out}...", file=sys.stderr, flush=True)
    out_path = args.out
    os.makedirs(os.path.dirname(out_path), exist_ok=True)
    with open(out_path, "w", encoding="utf-8") as f:
        f.write("\n".join(md))
    hist_dir = "tests/benchmarks/history"
    os.makedirs(hist_dir, exist_ok=True)
    stamp = hist["generated"]
    hist_path = os.path.join(hist_dir, f"bench_{stamp}.json")
    with open(hist_path, "w", encoding="utf-8") as f:
        json.dump(hist, f)
    print(f"[Progress] Done! Report saved to {args.out}", file=sys.stderr, flush=True)


if __name__ == "__main__":
    main()
