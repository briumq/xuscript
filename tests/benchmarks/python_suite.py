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
    print(f"loop_{N}={test_loop(N)}")
    print(f"dict_{N}={test_dict(N)}")
    print(f"dict_hot_{N}={test_dict_hot(N)}")
    print(f"dict_intkey_{N}={test_dict_intkey(N)}")
    print(f"string_{N}={test_string(N)}")
    print(f"string_builder_{N}={test_string_builder(N)}")
    print(f"struct_method_{N}={test_struct_method(N)}")
    print(f"try_catch_{N}={test_try_catch(N)}")
