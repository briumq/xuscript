import argparse
import time
import json

def gen_python_code(n: int) -> str:
    # generate sequential assignments: a0=0; a1=1; ...
    lines = [f"a{i}={i}" for i in range(n)]
    return "\n".join(lines)

def bench_json_parse(n: int):
    payload = {f"k{i}": i for i in range(n)}
    s = json.dumps(payload, ensure_ascii=False)
    t0 = time.perf_counter()
    obj = json.loads(s)
    t1 = time.perf_counter()
    return {"case": "json-parse", "scale": n, "result": len(obj), "duration_ms": (t1 - t0) * 1000.0}

def bench_ast_parse(n: int):
    import ast
    code = gen_python_code(n)
    t0 = time.perf_counter()
    tree = ast.parse(code)
    t1 = time.perf_counter()
    return {"case": "ast-parse", "scale": n, "result": len(getattr(tree, 'body', [])), "duration_ms": (t1 - t0) * 1000.0}

def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--scale", type=int, default=5000)
    args = ap.parse_args()
    out = [bench_json_parse(args.scale), bench_ast_parse(args.scale)]
    for item in out:
        print(json.dumps(item, ensure_ascii=False))

if __name__ == "__main__":
    main()

