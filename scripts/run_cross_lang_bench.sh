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

# Build Xu CLI once in release mode to avoid dev-profile noise
cargo build -q -p xu_cli --bin xu --release
XU_BIN="$(pwd)/target/release/xu"

echo "Python:"
if ! run_with_timeout python3 tests/benchmarks/python/bench.py --scale "$SCALE"; then
  echo "Python benchmark timed out or failed" >&2
fi
echo "Node.js:"
if ! run_with_timeout node tests/benchmarks/node/bench.js --scale "$SCALE"; then
  echo "Node benchmark timed out or failed" >&2
fi

# Run Xu suite
# Create wrapper to inject SCALE
WRAPPER="tests/benchmarks/xu/temp_bench.xu"
cat tests/benchmarks/xu/suite.xu > "$WRAPPER"
echo "BENCH_SCALE 为 \"$SCALE\"；" >> "$WRAPPER"
echo "BENCH_SMOKE 为 \"0\"；" >> "$WRAPPER"
echo "主程序()；" >> "$WRAPPER"
trap 'rm -f "$WRAPPER"' EXIT

# Run and capture output
# We capture stderr too just in case, but output is on stdout
OUT_FILE="$(mktemp -t xu_bench_out.XXXXXX)"
ERR_FILE="$(mktemp -t xu_bench_err.XXXXXX)"
trap 'rm -f "$WRAPPER" "$OUT_FILE" "$ERR_FILE"' EXIT

if ! run_with_timeout "$XU_BIN" run --no-diags "$WRAPPER" >"$OUT_FILE" 2>"$ERR_FILE"; then
  echo "Xu benchmark timed out or failed" >&2
  if [ -s "$ERR_FILE" ]; then
    cat "$ERR_FILE" >&2
  fi
fi
OUTPUT="$(cat "$OUT_FILE")"

# Helper to extract value
get_val() {
  local key="$1"
  local val=$(echo "$OUTPUT" | grep -F "${key}_${SCALE}=" | head -n1 | cut -d= -f2)
  if [ -z "$val" ]; then
    echo "N/A"
  else
    echo "$val"
  fi
}

echo "Xu loop:"
get_val "loop"
echo "Xu dict:"
get_val "dict"
echo "Xu dict-intkey:"
get_val "dict_intkey"
echo "Xu string:"
get_val "string"
echo "Xu string-builder:"
get_val "string_builder"
echo "Xu dict-hot:"
get_val "dict_hot"
echo "Xu struct-method:"
get_val "struct_method"
