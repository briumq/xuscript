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
  const [tLoop, rLoop] = ms(loopAccumulate, scale)
  const [tDict, rDict] = ms(dictBulk, scale)
  console.log(`JS|perf_runtime_loop_accumulate.exec_ms=${tLoop}|result=${rLoop}`)
  console.log(`JS|perf_runtime_bulk_dict_ops.exec_ms=${tDict}|result=${rDict}`)
}

main()
