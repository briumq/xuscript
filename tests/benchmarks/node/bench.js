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

function benchFuncCall(n) {
  function f(x) { return x + 1; }
  const t0 = nowMs();
  let s = 0;
  for (let i = 0; i < n; i++) s += f(i);
  const t1 = nowMs();
  return { case: "func-call", scale: n, result: s, duration_ms: Number(t1 - t0) / 1e6 };
}

function benchBranchHeavy(n) {
  const t0 = nowMs();
  let s = 0;
  for (let i = 0; i < n; i++) {
    if ((i & 1) === 0) s += 1;
    else s -= 1;
  }
  const t1 = nowMs();
  return { case: "branch-heavy", scale: n, result: s, duration_ms: Number(t1 - t0) / 1e6 };
}

function benchListPushPop(n) {
  const t0 = nowMs();
  const l = [];
  for (let i = 0; i < n; i++) l.push(i);
  for (let i = 0; i < n; i++) l.pop();
  const t1 = nowMs();
  return { case: "list-push-pop", scale: n, result: l.length, duration_ms: Number(t1 - t0) / 1e6 };
}

function benchDictMiss(n) {
  const d = {};
  for (let i = 0; i < n; i++) d["k" + i] = i;
  const t0 = nowMs();
  let s = 0;
  for (let i = 0; i < n; i++) s += (d["x" + i] || 0);
  const t1 = nowMs();
  return { case: "dict-miss", scale: n, result: s, duration_ms: Number(t1 - t0) / 1e6 };
}

function benchDictUpdateHot(n) {
  const d = { hot: 0 };
  const t0 = nowMs();
  for (let i = 0; i < n; i++) d.hot = i;
  const t1 = nowMs();
  return { case: "dict-update-hot", scale: n, result: d.hot, duration_ms: Number(t1 - t0) / 1e6 };
}

function benchStringUnicode(n) {
  const t0 = nowMs();
  const parts = ["こんにちは", "世界🌏", "Xu", "脚本"];
  const arr = new Array(n);
  for (let i = 0; i < n; i++) arr[i] = parts[i % parts.length];
  const s = arr.join(",");
  const out = s.split(",");
  const t1 = nowMs();
  return { case: "string-unicode", scale: n, result: out.length, duration_ms: Number(t1 - t0) / 1e6 };
}

function benchStringScan(n) {
  const s = new Array(n).fill(0).map((_, i) => String(i)).join(",");
  const t0 = nowMs();
  const c1 = s.includes("999");
  const c2 = s.startsWith("0,1");
  const c3 = s.endsWith(String(n - 1));
  const t1 = nowMs();
  return { case: "string-scan", scale: n, result: (c1 && c2 && c3) ? 1 : 0, duration_ms: Number(t1 - t0) / 1e6 };
}

function runCase(fn, n, warms, repeat) {
  for (let i = 0; i < warms; i++) fn(n);
  let best = null;
  let last = null;
  for (let i = 0; i < repeat; i++) {
    const item = fn(n);
    last = item;
    const d = item.duration_ms;
    if (best === null || d < best) best = d;
  }
  last.duration_ms = best;
  last.rss_bytes = process.memoryUsage().rss;
  return last;
}

const n = processArgs.scale;
const warms = parseInt(process.env.WARMS || "0", 10);
const repeat = parseInt(process.env.REPEAT || "1", 10);
const fns = [
  benchLoop,
  benchDict,
  benchDictIntKey,
  benchString,
  benchStringBuilder,
  benchDictHot,
  benchStructMethod,
  benchFuncCall,
  benchBranchHeavy,
  benchListPushPop,
  benchDictMiss,
  benchDictUpdateHot,
  benchStringUnicode,
  benchStringScan,
];
for (const fn of fns) {
  const item = runCase(fn, n, warms, repeat);
  console.log(JSON.stringify(item));
}
