import subprocess
import json
import statistics
import os

CASES = [
    ("loop", "Loop overhead"),
    ("dict", "Dict insert/get (str)"),
    ("dict-intkey", "Dict insert/get (int)"),
    ("string", "String concat"),
    ("string-builder", "StringBuilder"),
    ("dict-hot", "Dict hot access"),
    ("struct-method", "Struct method call"),
    ("func-call", "Function call"),
    ("branch-heavy", "Branch heavy"),
    ("try-catch", "Try-catch overhead"),
    ("list-push-pop", "List push/pop"),
    ("dict-miss", "Dict miss"),
    ("dict-update-hot", "Dict update hot"),
    ("string-unicode", "String unicode"),
    ("string-scan", "String scan"),
]

def run(cmd):
    return subprocess.run(cmd, capture_output=True, text=True)

def bench_scale(scale):
    p = run(["bash", "scripts/run_cross_lang_bench.sh", str(scale)])
    out = p.stdout.splitlines()
    # parse python/node/xu json lines
    py = {}
    nd = {}
    xu = {}
    current = None
    for line in out:
        line = line.strip()
        if not line:
            continue
        if line.startswith("Python:"):
            current = "py"
            continue
        if line.startswith("Node.js:"):
            current = "node"
            continue
        if line.startswith("Xu:"):
            current = "xu"
            continue
            
        if current in ["py", "node", "xu"]:
            try:
                if not line.startswith("{"):
                    continue
                obj = json.loads(line)
                case = obj.get("case")
                dur = obj.get("duration_ms")
                if current == "py":
                    py.setdefault(case, []).append(float(dur))
                elif current == "node":
                    nd.setdefault(case, []).append(float(dur))
                elif current == "xu":
                    xu.setdefault(case, []).append(float(dur))
            except Exception:
                pass
                
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

