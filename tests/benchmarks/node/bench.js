const processArgs = (() => {
  const out = { scale: 500 };
  for (let i = 2; i < process.argv.length; i++) {
    const a = process.argv[i];
    if (a === "--scale") out.scale = parseInt(process.argv[++i], 10);
  }
  return out;
})();

function nowMs() {
  return process.hrtime.bigint();
}

function benchLoop(n) {
  const t0 = nowMs();
  let s = 0;
  for (let i = 0; i < n; i++) s += 1;
  const t1 = nowMs();
  return { case: "loop", scale: n, result: s, duration_ms: Number(t1 - t0) / 1e6 };
}

function benchDict(n) {
  const t0 = nowMs();
  const d = {};
  for (let i = 0; i < n; i++) d["k" + i] = i;
  let s = 0;
  for (let i = 0; i < n; i++) s += d["k" + i];
  const t1 = nowMs();
  return { case: "dict", scale: n, result: s, duration_ms: Number(t1 - t0) / 1e6 };
}

function benchDictIntKey(n) {
  const t0 = nowMs();
  const d = {};
  for (let i = 0; i < n; i++) d[i] = i;
  let s = 0;
  for (let i = 0; i < n; i++) s += d[i];
  const t1 = nowMs();
  return { case: "dict-intkey", scale: n, result: s, duration_ms: Number(t1 - t0) / 1e6 };
}

function benchString(n) {
  const t0 = nowMs();
  const arr = new Array(n).fill(0).map((_, i) => String(i));
  const s = arr.join(",");
  const parts = s.split(",");
  const t1 = nowMs();
  return { case: "string", scale: n, result: parts.length, duration_ms: Number(t1 - t0) / 1e6 };
}

function benchStringBuilder(n) {
  const t0 = nowMs();
  const parts = new Array(n);
  for (let i = 0; i < n; i++) parts[i] = String(i);
  const s = parts.join(",");
  const t1 = nowMs();
  return { case: "string-builder", scale: n, result: s.length, duration_ms: Number(t1 - t0) / 1e6 };
}

function benchDictHot(n) {
  const d = {};
  for (let i = 0; i < n; i++) d["k" + i] = i;
  const hot = "k" + (n >> 1);
  const t0 = nowMs();
  let s = 0;
  for (let i = 0; i < n; i++) s += d[hot];
  const t1 = nowMs();
  return { case: "dict-hot", scale: n, result: s, duration_ms: Number(t1 - t0) / 1e6 };
}

class Obj {
  constructor() { this.base = 1; }
  method(x) { return x + this.base; }
}

function benchStructMethod(n) {
  const o = new Obj();
  const t0 = nowMs();
  let s = 0;
  for (let i = 0; i < n; i++) s += o.method(i);
  const t1 = nowMs();
  return { case: "struct-method", scale: n, result: s, duration_ms: Number(t1 - t0) / 1e6 };
}

const n = processArgs.scale;
const out = [
  benchLoop(n),
  benchDict(n),
  benchDictIntKey(n),
  benchString(n),
  benchStringBuilder(n),
  benchDictHot(n),
  benchStructMethod(n),
];
for (const item of out) console.log(JSON.stringify(item));
