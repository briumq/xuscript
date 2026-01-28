import argparse
import time
import json

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

def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--scale", type=int, default=500)
    args = ap.parse_args()
    out = [
        bench_loop(args.scale),
        bench_dict(args.scale),
        bench_dict_intkey(args.scale),
        bench_string(args.scale),
        bench_string_builder(args.scale),
        bench_dict_hot(args.scale),
        bench_struct_method(args.scale),
    ]
    for item in out:
        print(json.dumps(item, ensure_ascii=False))

if __name__ == "__main__":
    main()
