#!/usr/bin/env bash
set -euo pipefail

SCALE=${1:-500}
TIMEOUT_SEC=${XU_BENCH_TIMEOUT_SEC:-60}

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
cat tests/benchmarks/xu/full_suite.xu > "$WRAPPER"
echo "" >> "$WRAPPER"
echo "BENCH_SCALE = \"$SCALE\"" >> "$WRAPPER"
echo "BENCH_SMOKE = \"0\"" >> "$WRAPPER"
OUT_XU="$(mktemp -t xu_bench_out.XXXXXX)"
ERR_XU="$(mktemp -t xu_bench_err.XXXXXX)"
trap 'rm -f "$WRAPPER" "$OUT_XU" "$ERR_XU"' EXIT
run_with_timeout "$XU_BIN" run --no-diags "$WRAPPER" >"$OUT_XU" 2>"$ERR_XU" || echo "Xu benchmark timed out or failed" >&2
cat "$OUT_XU"
