import time


def main():
    size = 200
    A = {f"k{i}": i for i in range(size)}
    B = {f"k{i}": i + 1 for i in range(size)}

    warm = 2000
    for _ in range(warm):
        A.update(B)

    iters = 200000
    t0 = time.perf_counter_ns()
    for _ in range(iters):
        A.update(B)
    t1 = time.perf_counter_ns()

    total = t1 - t0
    per = total // iters
    print(f"lang=python iters={iters} total_ns={total} per_ns={per}")


if __name__ == "__main__":
    main()
