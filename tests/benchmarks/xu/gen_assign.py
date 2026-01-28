import argparse

def gen_xu_assign(n: int) -> str:
    # Xu uses Chinese terminator ； in benchmarks; use it here
    lines = [f"a{i} 为 {i}；" for i in range(n)]
    return "\n".join(lines) + "\n"

def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--scale", type=int, default=5000)
    ap.add_argument("--out", type=str, required=True)
    args = ap.parse_args()
    s = gen_xu_assign(args.scale)
    with open(args.out, "w", encoding="utf-8") as f:
        f.write(s)

if __name__ == "__main__":
    main()
