import time
import os

def run(name, repeat, logic):
    min_time = float('inf')
    for _ in range(repeat):
        start = time.perf_counter()
        logic()
        end = time.perf_counter()
        duration = (end - start) * 1000 # convert to ms
        if duration < min_time:
            min_time = duration
    return round(min_time)

def aggregate(label, fn, N, runs=5):
    vals = []
    for _ in range(runs):
        vals.append(fn(N))
    vals.sort()
    def p95(xs):
        if not xs: return 0
        idx = max(0, int(0.95 * (len(xs) - 1)))
        return xs[idx]
    print(f"{label}_{N}=min:{vals[0]} median:{vals[len(vals)//2]} p95:{p95(vals)} max:{vals[-1]}")

def test_loop(N):
    def logic():
        count = 0
        for r in range(1, N + 1):
            count += 1
    return run(f"loop_{N}", 7, logic)

def test_dict(N):
    def logic():
        d = {}
        for i in range(N):
            k = "k" + str(i)
            d[k] = i
        total = 0
        for i in range(N):
            k = "k" + str(i)
            total += d[k]
    return run(f"dict_{N}", 7, logic)

def test_dict_hot(N):
    def logic():
        d = {}
        for i in range(N):
            d[str(i)] = i
        for j in range(N):
            _v = d["0"]
    return run(f"dict_hot_{N}", 7, logic)

def test_dict_intkey(N):
    def logic():
        d = {}
        for i in range(N):
            d[i] = i
        total = 0
        for i in range(N):
            total += d[i]
    return run(f"dict_intkey_{N}", 7, logic)

def test_string(N):
    def logic():
        s = ""
        for i in range(N):
            s += str(i)
            s += ","
    return run(f"string_{N}", 7, logic)

def test_string_builder(N):
    def logic():
        b = []
        for i in range(N):
            b.append(str(i))
            b.append(",")
        s = "".join(b)
    return run(f"string_builder_{N}", 7, logic)

class Foo:
    def __init__(self, x):
        self.x = x
    def add(self, n):
        return self.x + n

def test_struct_method(N):
    def logic():
        f = Foo(1)
        for i in range(N):
            _v = f.add(i)
    return run(f"struct_method_{N}", 7, logic)

def test_try_catch(N):
    def logic():
        placeholder = 0
        for i in range(1, N + 1):
            try:
                raise Exception("e")
            except Exception as e:
                if len(str(e)) > 0:
                    placeholder = 0
            finally:
                placeholder = 0
    return run(f"try_catch_{N}", 7, logic)

if __name__ == "__main__":
    N = int(os.environ.get("BENCH_SCALE", 10000))
    runs = int(os.environ.get("BENCH_RUNS", 1))
    aggregate("loop", test_loop, N, runs)
    aggregate("dict", test_dict, N, runs)
    aggregate("dict_hot", test_dict_hot, N, runs)
    aggregate("dict_intkey", test_dict_intkey, N, runs)
    aggregate("string", test_string, N, runs)
    aggregate("string_builder", test_string_builder, N, runs)
    aggregate("struct_method", test_struct_method, N, runs)
    aggregate("try_catch", test_try_catch, N, runs)
