function nowMs() {
  return Number(process.hrtime.bigint() / 1000000n);
}

function benchJsonParse(n) {
  const payload = {};
  for (let i = 0; i < n; i++) payload["k" + i] = i;
  const s = JSON.stringify(payload);
  const t0 = nowMs();
  const obj = JSON.parse(s);
  const t1 = nowMs();
  return { case: "json-parse", scale: n, result: Object.keys(obj).length, duration_ms: t1 - t0 };
}

function genJsCode(n) {
  let out = "";
  for (let i = 0; i < n; i++) out += `var a${i}=${i};\n`;
  return out;
}

function benchCompileFunction(n) {
  const code = genJsCode(n);
  const t0 = nowMs();
  // new Function compiles the code; we do not execute it.
  const fn = new Function(code);
  const t1 = nowMs();
  return { case: "compile-fn", scale: n, result: n, duration_ms: t1 - t0 };
}

const args = (() => {
  let scale = 5000;
  for (let i = 2; i < process.argv.length; i++) {
    if (process.argv[i] === "--scale") scale = parseInt(process.argv[++i], 10);
  }
  return { scale };
})();

const n = args.scale;
const out = [benchJsonParse(n), benchCompileFunction(n)];
for (const item of out) console.log(JSON.stringify(item));

