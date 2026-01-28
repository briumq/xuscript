import subprocess
import json
import statistics
import re
import os

def run(cmd):
    return subprocess.run(cmd, capture_output=True, text=True)

def ensure_xu_bin():
    subprocess.run(["cargo","build","-q","-p","xu_cli","--bin","xu","--release"], check=True)
    return os.path.abspath("target/release/xu")

def gen_xu_src(scale, path):
    subprocess.run(["python3","tests/benchmarks/xu/gen_assign.py","--scale",str(scale),"--out",path], check=True)

def parse_python(scale):
    p = run(["python3","tests/benchmarks/python/parse.py","--scale",str(scale)])
    out = [json.loads(l) for l in p.stdout.strip().splitlines() if l.strip()]
    m = {o["case"]: o for o in out}
    return m.get("json-parse",{}).get("duration_ms",-1.0), m.get("ast-parse",{}).get("duration_ms",-1.0)

def parse_node(scale):
    p = run(["node","tests/benchmarks/node/parse.js","--scale",str(scale)])
    out = [json.loads(l) for l in p.stdout.strip().splitlines() if l.strip()]
    m = {o["case"]: o for o in out}
    return m.get("json-parse",{}).get("duration_ms",-1.0), m.get("compile-fn",{}).get("duration_ms",-1.0)

def parse_xu(scale, xu_bin, xu_src):
    p = run([xu_bin,"ast","--timing",xu_src])
    m = re.search(r"TIMING normalize=([0-9.]+)ms lex=([0-9.]+)ms parse=([0-9.]+)ms analyze=([0-9.]+)ms", p.stdout)
    return float(m.group(3)) if m else -1.0

def stats(vals):
    vals = [v for v in vals if v >= 0]
    if not vals:
        return {"min": -1.0, "median": -1.0}
    return {"min": min(vals), "median": statistics.median(vals)}

def gen_xu_json_prog(scale, path):
    import json as pyjson
    obj = {f"k{i}": i for i in range(scale)}
    s = pyjson.dumps(obj, ensure_ascii=False)
    s = s.replace('{', '\\{').replace('}', '\\}')
    prog = (
        "引入(\"std/json\");\n"
        "定义 主程序():\n"
        f"  文 为 \"{s}\"；\n"
        "  开 为 单调微秒();\n"
        "  R 为 解析JSON(文)；\n"
        "  关 为 单调微秒();\n"
        f"  如果 R 的 长度 不是 {scale}：\n"
        "    输出(\"校验失败：长度不匹配\")；\n"
        "    返回；\n"
        "  如果 R[\"k0\"] 不是 0：\n"
        "    输出(\"校验失败：k0 不匹配\")；\n"
        "    返回；\n"
        f"  如果 R[\"k{scale-1}\"] 不是 {scale-1}：\n"
        "    输出(\"校验失败：尾键不匹配\")；\n"
        "    返回；\n"
        "  输出((关 - 开 + 500) / 1000)；\n"
        "  输出(\"校验通过\")；\n"
    )
    with open(path, 'w', encoding='utf-8') as f:
        f.write(prog)

def parse_xu_json(scale, xu_bin, prog):
    p = run([xu_bin, "run", prog])
    import re
    m = re.findall(r"[0-9]+(?:\.[0-9]+)?", p.stdout)
    return float(m[-1]) if m else -1.0

def main():
    scales = [1000, 5000, 10000, 20000, 50000, 100000, 200000, 500000]
    runs = 5
    xu_bin = ensure_xu_bin()
    os.makedirs("tmp", exist_ok=True)
    rows = []
    for s in scales:
        xu_src = f"tmp/parse_xu_{s}.xu"
        gen_xu_src(s, xu_src)
        py_json, py_ast = [], []
        node_json, node_compile = [], []
        xu_ast = []
        xu_json = []
        for _ in range(runs):
            pj, pa = parse_python(s)
            nj, nc = parse_node(s)
            xa = parse_xu(s, xu_bin, xu_src)
            # Xu JSON parse
            xu_json_prog = f"tmp/xu_json_parse_{s}.xu"
            gen_xu_json_prog(s, xu_json_prog)
            xj = parse_xu_json(s, xu_bin, xu_json_prog)
            py_json.append(pj)
            py_ast.append(pa)
            node_json.append(nj)
            node_compile.append(nc)
            xu_ast.append(xa)
            xu_json.append(xj)
        rows.append({
            "scale": s,
            "python_json": stats(py_json),
            "python_ast": stats(py_ast),
            "node_json": stats(node_json),
            "node_compile": stats(node_compile),
            "xu_ast": stats(xu_ast),
            "xu_json": stats(xu_json),
        })
    md = []
    md.append("# 解析性能对比多轮（min/median）")
    md.append("")
    for r in rows:
        md.append(f"## N={r['scale']}")
        md.append(f"- Python json-parse: min {float(r['python_json']['min']):.3}ms, median {float(r['python_json']['median']):.3}ms")
        md.append(f"- Python ast-parse: min {float(r['python_ast']['min']):.3}ms, median {float(r['python_ast']['median']):.3}ms")
        md.append(f"- Node JSON.parse: min {float(r['node_json']['min']):.3}ms, median {float(r['node_json']['median']):.3}ms")
        md.append(f"- Node compile-fn: min {float(r['node_compile']['min']):.3}ms, median {float(r['node_compile']['median']):.3}ms")
        md.append(f"- Xu ast: min {float(r['xu_ast']['min']):.3}ms, median {float(r['xu_ast']['median']):.3}ms")
        md.append(f"- Xu json-parse: min {float(r['xu_json']['min']):.3}ms, median {float(r['xu_json']['median']):.3}ms")
        md.append("")
    os.makedirs("docs", exist_ok=True)
    with open("docs/解析性能对比_多轮_2026-01-22.md","w",encoding="utf-8") as f:
        f.write("\n".join(md))

if __name__ == "__main__":
    main()
