function main() {
  const size = 200;
  const A = {};
  const B = {};
  for (let i = 0; i < size; i++) {
    A[`k${i}`] = i;
    B[`k${i}`] = i + 1;
  }

  const warm = 2000;
  for (let i = 0; i < warm; i++) {
    Object.assign(A, B);
  }

  const iters = 200000;
  const t0 = process.hrtime.bigint();
  for (let i = 0; i < iters; i++) {
    Object.assign(A, B);
  }
  const t1 = process.hrtime.bigint();

  const total = t1 - t0;
  const per = total / BigInt(iters);
  console.log(`lang=node iters=${iters} total_ns=${total} per_ns=${per}`);
}

main();
