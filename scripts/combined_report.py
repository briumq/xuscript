import subprocess
import os
import json

def run(cmd):
    return subprocess.run(cmd, capture_output=True, text=True)

def run_xu(file):
    p = run(["target/release/xu", "run", file])
    return p.stdout.strip()

def main():
    subprocess.run(["cargo","build","-q","-p","xu_cli","--bin","xu","--release"], check=True)
    os.makedirs("docs", exist_ok=True)

    # Parse report is generated separately
    subprocess.run(["python3","scripts/parse_report.py"], check=True)

    # Exec report (Python/Node/Xu)
    subprocess.run(["python3","scripts/bench_report.py"], check=True)

    # Extra Xu dimensions
    rows = []
    rows.append(("Xu import-cache 首次/后续(ms)", run_xu("tests/benchmarks/xu/bench_import_cache.xu")))
    rows.append(("Xu string-builder 大规模(ms)", run_xu("tests/benchmarks/xu/bench_string_builder_large.xu")))
    rows.append(("Xu try-catch 抛出/捕获(ms)", run_xu("tests/benchmarks/xu/bench_try_catch.xu")))
    rows.append(("Xu json-parse 多规模(ms)", run_xu("tests/benchmarks/xu/bench_json.xu")))

    md = []
    md.append("# 综合性能报告（执行/解析/扩展维度）")
    md.append("")
    md.append("- 执行基准：见 最终性能报告_执行与解析_2026-01-22.md")
    md.append("- 解析基准：见 解析性能对比_多轮_2026-01-22.md")
    md.append("")
    md.append("## 扩展维度（Xu 自写脚本）")
    for title, out in rows:
        md.append(f"### {title}")
        for line in out.splitlines():
            md.append(f"- {line}")
        md.append("")
    with open("docs/综合性能报告_执行解析与扩展维度_2026-01-22.md","w",encoding="utf-8") as f:
        f.write("\n".join(md))

if __name__ == "__main__":
    main()

