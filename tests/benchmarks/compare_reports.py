import argparse
import json
import math

def load(path):
  with open(path, "r", encoding="utf-8") as f:
    return json.load(f)

def ratio(a, b):
  if not math.isfinite(a) or not math.isfinite(b) or b == 0:
    return float("nan")
  return a / b

def main():
  ap = argparse.ArgumentParser()
  ap.add_argument("--old", required=True)
  ap.add_argument("--new", required=True)
  args = ap.parse_args()
  old = load(args.old)
  new = load(args.new)
  scales = set(s["scale"] for s in old["results"]) & set(s["scale"] for s in new["results"])
  print("| scale | case | py Δ% | node Δ% | xu Δ% |")
  print("|---:|---|---:|---:|---:|")
  for s in sorted(scales):
    ro = next(r for r in old["results"] if r["scale"] == s)
    rn = next(r for r in new["results"] if r["scale"] == s)
    cases = set(ro["cases"].keys()) | set(rn["cases"].keys())
    for c in sorted(cases):
      po = ro["cases"].get(c, {}).get("py")
      pn = rn["cases"].get(c, {}).get("py")
      no = ro["cases"].get(c, {}).get("node")
      nn = rn["cases"].get(c, {}).get("node")
      xo = ro["cases"].get(c, {}).get("xu")
      xn = rn["cases"].get(c, {}).get("xu")
      dp = ratio(pn, po)
      dn = ratio(nn, no)
      dx = ratio(xn, xo)
      def fmt(r):
        return "-" if not math.isfinite(r) else f"{(r - 1.0) * 100.0:.1f}%"
      print(f"| {s} | {c} | {fmt(dp)} | {fmt(dn)} | {fmt(dx)} |")

if __name__ == "__main__":
  main()
