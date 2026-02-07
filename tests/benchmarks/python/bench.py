import argparse
import time
import json
import os
import resource

def bench_loop(n):
    t0 = time.perf_counter()
    s = 0
    for i in range(n):
        s += 1
    t1 = time.perf_counter()
    return {"case": "loop", "scale": n, "result": s, "duration_ms": (t1 - t0) * 1000.0}

def bench_dict(n):
    t0 = time.perf_counter()
    d = {}
    for i in range(n):
        d[f"k{i}"] = i
    s = 0
    for i in range(n):
        s += d[f"k{i}"]
    t1 = time.perf_counter()
    return {"case": "dict", "scale": n, "result": s, "duration_ms": (t1 - t0) * 1000.0}

def bench_string(n):
    t0 = time.perf_counter()
    arr = [str(i) for i in range(n)]
    s = ",".join(arr)
    parts = s.split(",")
    t1 = time.perf_counter()
    return {"case": "string", "scale": n, "result": len(parts), "duration_ms": (t1 - t0) * 1000.0}

def bench_dict_intkey(n):
    t0 = time.perf_counter()
    d = {}
    for i in range(n):
        d[i] = i
    s = 0
    for i in range(n):
        s += d[i]
    t1 = time.perf_counter()
    return {"case": "dict-intkey", "scale": n, "result": s, "duration_ms": (t1 - t0) * 1000.0}

def bench_string_builder(n):
    t0 = time.perf_counter()
    parts = []
    for i in range(n):
        parts.append(str(i))
    s = ",".join(parts)
    t1 = time.perf_counter()
    return {"case": "string-builder", "scale": n, "result": len(s), "duration_ms": (t1 - t0) * 1000.0}

def bench_dict_hot(n):
    d = {f"k{i}": i for i in range(n)}
    hot = f"k{n//2}"
    t0 = time.perf_counter()
    s = 0
    for _ in range(n):
        s += d[hot]
    t1 = time.perf_counter()
    return {"case": "dict-hot", "scale": n, "result": s, "duration_ms": (t1 - t0) * 1000.0}

class Obj:
    def __init__(self):
        self.base = 1
    def method(self, x):
        return x + self.base

def bench_struct_method(n):
    o = Obj()
    t0 = time.perf_counter()
    s = 0
    for i in range(n):
        s += o.method(i)
    t1 = time.perf_counter()
    return {"case": "struct-method", "scale": n, "result": s, "duration_ms": (t1 - t0) * 1000.0}

def bench_func_call(n):
    def f(x): return x + 1
    t0 = time.perf_counter()
    s = 0
    for i in range(n):
        s += f(i)
    t1 = time.perf_counter()
    return {"case": "func-call", "scale": n, "result": s, "duration_ms": (t1 - t0) * 1000.0}

def bench_branch_heavy(n):
    t0 = time.perf_counter()
    s = 0
    for i in range(n):
        if (i & 1) == 0:
            s += 1
        else:
            s -= 1
    t1 = time.perf_counter()
    return {"case": "branch-heavy", "scale": n, "result": s, "duration_ms": (t1 - t0) * 1000.0}

def bench_list_push_pop(n):
    t0 = time.perf_counter()
    l = []
    for i in range(n):
        l.append(i)
    for i in range(n):
        l.pop()
    t1 = time.perf_counter()
    return {"case": "list-push-pop", "scale": n, "result": len(l), "duration_ms": (t1 - t0) * 1000.0}

def bench_dict_miss(n):
    d = {f"k{i}": i for i in range(n)}
    t0 = time.perf_counter()
    s = 0
    for i in range(n):
        s += d.get(f"x{i}", 0)
    t1 = time.perf_counter()
    return {"case": "dict-miss", "scale": n, "result": s, "duration_ms": (t1 - t0) * 1000.0}

def bench_dict_update_hot(n):
    d = {"hot": 0}
    t0 = time.perf_counter()
    for i in range(n):
        d["hot"] = i
    t1 = time.perf_counter()
    return {"case": "dict-update-hot", "scale": n, "result": d["hot"], "duration_ms": (t1 - t0) * 1000.0}

def bench_string_unicode(n):
    t0 = time.perf_counter()
    parts = ["こんにちは", "世界🌏", "Xu", "脚本"] * (n // 4)
    s = ",".join(parts)
    out = s.split(",")
    t1 = time.perf_counter()
    return {"case": "string-unicode", "scale": n, "result": len(out), "duration_ms": (t1 - t0) * 1000.0}

def bench_string_scan(n):
    s = ",".join([str(i) for i in range(n)])
    t0 = time.perf_counter()
    c1 = "999" in s
    c2 = s.startswith("0,1")
    c3 = s.endswith(str(n - 1))
    t1 = time.perf_counter()
    return {"case": "string-scan", "scale": n, "result": int(c1 and c2 and c3), "duration_ms": (t1 - t0) * 1000.0}

def bench_closure_create(n):
    t0 = time.perf_counter()
    total = 0
    for i in range(n):
        captured = i
        f = lambda x, c=captured: x + c
        total += f(1)
    t1 = time.perf_counter()
    return {"case": "closure-create", "scale": n, "result": total, "duration_ms": (t1 - t0) * 1000.0}

def bench_closure_call(n):
    captured = 42
    f = lambda x: x + captured
    t0 = time.perf_counter()
    total = 0
    for i in range(n):
        total += f(i)
    t1 = time.perf_counter()
    return {"case": "closure-call", "scale": n, "result": total, "duration_ms": (t1 - t0) * 1000.0}

def run_case(fn, n, warms, repeat):
    for _ in range(warms):
        fn(n)
    total = 0.0
    last = None
    for _ in range(repeat):
        item = fn(n)
        last = item
        total += item["duration_ms"]
    rss = resource.getrusage(resource.RUSAGE_SELF).ru_maxrss
    last["duration_ms"] = total / repeat
    last["rss_bytes"] = int(rss) if isinstance(rss, (int, float)) else 0
    return last

def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--scale", type=int, default=500)
    args = ap.parse_args()
    warms = int(os.environ.get("WARMS", "0") or "0")
    repeat = int(os.environ.get("REPEAT", "1") or "1")
    fns = [
        bench_loop,
        bench_dict,
        bench_dict_intkey,
        bench_string,
        bench_string_builder,
        bench_dict_hot,
        bench_struct_method,
        bench_func_call,
        bench_branch_heavy,
        bench_list_push_pop,
        bench_dict_miss,
        bench_dict_update_hot,
        bench_string_unicode,
        bench_string_scan,
        bench_closure_create,
        bench_closure_call,
    ]
    for fn in fns:
        item = run_case(fn, args.scale, warms, repeat)
        print(json.dumps(item, ensure_ascii=False))

if __name__ == "__main__":
    main()
