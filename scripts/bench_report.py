import subprocess
import json
import statistics
import os

CASES = [
    ("loop", "Xu loop:"),
    ("dict", "Xu dict:"),
    ("dict-intkey", "Xu dict-intkey:"),
    ("string", "Xu string:"),
    ("string-builder", "Xu string-builder:"),
    ("dict-hot", "Xu dict-hot:"),
    ("struct-method", "Xu struct-method:"),
]

def run(cmd):
    return subprocess.run(cmd, capture_output=True, text=True)

def bench_scale(scale):
    p = run(["bash", "scripts/run_cross_lang_bench.sh", str(scale)])
    out = p.stdout.splitlines()
    # parse python/node json lines
    py = {}
    nd = {}
    xu = {}
    current = None
    for line in out:
        if line.startswith("Python:"):
            current = "py"
            continue
        if line.startswith("Node.js:"):
            current = "node"
            continue
        if any(line.startswith(tag) for _, tag in CASES):
            current = "xu"
            continue
        if current == "py" or current == "node":
            try:
                obj = json.loads(line)
                case = obj.get("case")
                dur = obj.get("duration_ms")
                if current == "py":
                    py.setdefault(case, []).append(float(dur))
                else:
                    nd.setdefault(case, []).append(float(dur))
            except Exception:
                pass
        elif current == "xu":
            # Xu prints a single number in ms per case; collect in order
            for case, tag in CASES:
                if tag in p.stdout:
                    # split by tag blocks and pick the number following tag
                    pass
            # simpler: scan numbers in order after each tag
            # rebuild mapping
            buf = p.stdout
            vals = {}
            for case, tag in CASES:
                idx = buf.find(tag)
                if idx == -1:
                    continue
                rest = buf[idx+len(tag):]
                # find first number line
                for ln in rest.splitlines():
                    ln = ln.strip()
                    if ln and ln[0].isdigit():
                        try:
                            vals[case] = float(ln)
                        except Exception:
                            pass
                        break
            xu = {k: [v] for k, v in vals.items()}
            break
    return py, nd, xu

def stats(vals):
    if not vals:
        return {"min": -1.0, "median": -1.0}
    return {"min": min(vals), "median": statistics.median(vals)}

def main():
    scales = [5000, 10000]
    runs = 3
    rows = []
    for s in scales:
        agg_py = {}
        agg_nd = {}
        agg_xu = {}
        for _ in range(runs):
            py, nd, xu = bench_scale(s)
            for k, v in py.items():
                agg_py.setdefault(k, []).extend(v)
            for k, v in nd.items():
                agg_nd.setdefault(k, []).extend(v)
            for k, v in xu.items():
                agg_xu.setdefault(k, []).extend(v)
        rows.append({
            "scale": s,
            "python": {k: stats(v) for k, v in agg_py.items()},
            "node": {k: stats(v) for k, v in agg_nd.items()},
            "xu": {k: stats(v) for k, v in agg_xu.items()},
        })
    os.makedirs("docs", exist_ok=True)
    md = []
    md.append("# 执行与解析性能报告（简版）")
    md.append("")
    md.append("## 解析（参考）详见 解析性能对比_多轮_2026-01-22.md")
    md.append("")
    for r in rows:
        md.append(f"## 执行 N={r['scale']}")
        # Python
        for case in [c for c,_ in CASES if c in r['python']]:
            m = r['python'][case]
            md.append(f"- Python {case}: min {m['min']:.3}ms, median {m['median']:.3}ms")
        # Node
        for case in [c for c,_ in CASES if c in r['node']]:
            m = r['node'][case]
            md.append(f"- Node {case}: min {m['min']:.3}ms, median {m['median']:.3}ms")
        # Xu
        for case in [c for c,_ in CASES if c in r['xu']]:
            m = r['xu'][case]
            md.append(f"- Xu {case}: min {m['min']:.3}ms, median {m['median']:.3}ms")
        md.append("")
    with open("docs/最终性能报告_执行与解析_2026-01-22.md","w",encoding="utf-8") as f:
        f.write("\n".join(md))

if __name__ == "__main__":
    main()

