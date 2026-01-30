#!/usr/bin/env bash
set -euo pipefail

SCALE=${1:-500}
TIMEOUT_SEC=${XU_BENCH_TIMEOUT_SEC:-60}
MAX_MEMORY_KB=${XU_BENCH_MAX_MEMORY_KB:-2097152}  # 默认 2GB

# 打印防护信息
echo "[Guard] Timeout: ${TIMEOUT_SEC}s, Memory limit: $((MAX_MEMORY_KB / 1024))MB" >&2

run_with_timeout() {
  perl -e '
    use POSIX qw(setpgid);
    my $t = shift @ARGV;
    my $pid = fork();
    die "fork failed" unless defined $pid;
    if ($pid == 0) {
      setpgid(0, 0);
      exec @ARGV;
      exit 127;
    }
    $SIG{ALRM} = sub {
      print STDERR "[Guard] Killing process group $pid due to timeout\n";
      kill "TERM", -$pid;
      select(undef, undef, undef, 0.5);
      kill "KILL", -$pid;
      exit 124;
    };
    alarm $t;
    waitpid($pid, 0);
    alarm 0;
    my $code = $? >> 8;
    exit $code;
  ' "$TIMEOUT_SEC" "$@"
}

# Locate Xu CLI binary; build only if missing
XU_BIN="$(pwd)/target/release/xu"
if [ ! -x "$XU_BIN" ]; then
  cargo build -q -p xu_cli --bin xu --release || true
fi

echo "Python:"
if ! WARMS="${WARMS:-0}" REPEAT="${REPEAT:-1}" run_with_timeout python3 tests/benchmarks/python/bench.py --scale "$SCALE"; then
  echo "Python benchmark timed out or failed" >&2
fi
echo "Node.js:"
if ! WARMS="${WARMS:-0}" REPEAT="${REPEAT:-1}" run_with_timeout node tests/benchmarks/node/bench.js --scale "$SCALE"; then
  echo "Node benchmark timed out or failed" >&2
fi

echo "Xu:"
# Create wrapper to inject scale and drive suite.xu 主程序
WRAPPER="tests/benchmarks/xu/temp_suite.xu"
# 使用 sed 替换 BENCH_SCALE 的默认值
sed "s/var BENCH_SCALE = \"5000\"/var BENCH_SCALE = \"$SCALE\"/" tests/benchmarks/xu/full_suite.xu > "$WRAPPER"
OUT_XU="$(mktemp -t xu_bench_out.XXXXXX)"
ERR_XU="$(mktemp -t xu_bench_err.XXXXXX)"
trap 'rm -f "$WRAPPER" "$OUT_XU" "$ERR_XU"' EXIT

# 使用 ulimit 限制内存（如果支持）
if ulimit -v "$MAX_MEMORY_KB" 2>/dev/null; then
  echo "[Guard] Memory limit applied to Xu process" >&2
fi

run_with_timeout "$XU_BIN" run --no-diags "$WRAPPER" >"$OUT_XU" 2>"$ERR_XU" || {
  echo "Xu benchmark timed out or failed" >&2
  # 打印错误输出以便调试
  if [ -s "$ERR_XU" ]; then
    echo "[Guard] Xu stderr:" >&2
    head -20 "$ERR_XU" >&2
  fi
}
cat "$OUT_XU"
