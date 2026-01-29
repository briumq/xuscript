function loopAccumulate(n) {
  let c = 0
  for (let i = 0; i < n; i++) c += 1
  return c
}

function dictBulk(n) {
  const d = {}
  for (let i = 0; i < n; i++) {
    const k = "k" + i
    d[k] = 1
  }
  return Object.keys(d).length
}

function ms(fn, arg) {
  const t0 = process.hrtime.bigint()
  const r = fn(arg)
  const t1 = process.hrtime.bigint()
  const durMs = Number(t1 - t0) / 1e6
  return [Math.round(durMs), r]
}

function main() {
  const scale = parseInt(process.env.BENCH_SCALE || "50000", 10)
  const runs = parseInt(process.env.BENCH_RUNS || "1", 10)
  function aggregate(label, fn, n, runs) {
    const vals = []
    for (let i = 0; i < runs; i++) {
      const [ms] = ms(fn, n)
      vals.push(ms)
    }
    vals.sort((a, b) => a - b)
    const p95 = vals[Math.max(0, Math.floor(0.95 * (vals.length - 1)))]
    console.log(`${label}_${n}=min:${vals[0]} median:${vals[Math.floor(vals.length/2)]} p95:${p95} max:${vals[vals.length-1]}`)
  }
  aggregate("loop_accumulate", loopAccumulate, scale, runs)
  aggregate("bulk_dict_ops", dictBulk, scale, runs)
}

main()
