import argparse
import json
import math
import os

def load(path):
  with open(path, "r", encoding="utf-8") as f:
    return json.load(f)

def latest_history(dirpath):
  files = [f for f in os.listdir(dirpath) if f.startswith("bench_") and f.endswith(".json")]
  if not files:
    raise SystemExit("No history files")
  files.sort()
  return os.path.join(dirpath, files[-1])

def main():
  ap = argparse.ArgumentParser()
  ap.add_argument("--threshold", type=float, default=10.0)
  ap.add_argument("--history", default="tests/benchmarks/history")
  args = ap.parse_args()
  latest = load(latest_history(args.history))
  prev_files = [f for f in os.listdir(args.history) if f.startswith("bench_") and f.endswith(".json")]
  prev_files.sort()
  if len(prev_files) < 2:
    print("Insufficient history, skipping gate")
    return 0
  prev = load(os.path.join(args.history, prev_files[-2]))
  scales = set(s["scale"] for s in prev["results"]) & set(s["scale"] for s in latest["results"])
  bad = []
  keys = ["dict", "dict-hot", "string-builder", "string-scan"]
  for s in sorted(scales):
    ro = next(r for r in prev["results"] if r["scale"] == s)
    rn = next(r for r in latest["results"] if r["scale"] == s)
    for k in keys:
      po = ro["cases"].get(k, {}).get("py")
      pn = rn["cases"].get(k, {}).get("py")
      no = ro["cases"].get(k, {}).get("node")
      nn = rn["cases"].get(k, {}).get("node")
      xo = ro["cases"].get(k, {}).get("xu")
      xn = rn["cases"].get(k, {}).get("xu")
      def rise(o, n):
        if o is None or n is None or not math.isfinite(o) or not math.isfinite(n) or o <= 0:
          return 0.0
        return (n / o - 1.0) * 100.0
      rp, rn_, rx = rise(po, pn), rise(no, nn), rise(xo, xn)
      if rp > args.threshold or rn_ > args.threshold or rx > args.threshold:
        bad.append((s, k, rp, rn_, rx))
  if bad:
    print("Perf gate failed:")
    for s, k, rp, rn_, rx in bad:
      print(f"- scale={s} case={k} Δ% py={rp:.1f} node={rn_:.1f} xu={rx:.1f}")
    raise SystemExit(1)
  print("Perf gate passed")
  return 0

if __name__ == "__main__":
  raise SystemExit(main())
