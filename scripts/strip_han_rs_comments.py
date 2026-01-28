from __future__ import annotations

import re
from pathlib import Path


HAN_RE = re.compile(r"[\u3400-\u4DBF\u4E00-\u9FFF\uF900-\uFAFF]")


def has_han(s: str) -> bool:
    return HAN_RE.search(s) is not None


def process_line(line: str) -> str | None:
    if not has_han(line):
        return line

    stripped = line.lstrip()
    if stripped.startswith("//"):
        return None

    if "//" in line:
        prefix = line.split("//", 1)[0].rstrip()
        if not prefix:
            return None
        return prefix + "\n"

    return line


def process_file(path: Path) -> tuple[bool, int]:
    original = path.read_text(encoding="utf-8")
    changed = False
    removed = 0
    out_lines: list[str] = []
    for line in original.splitlines(keepends=True):
        new_line = process_line(line)
        if new_line is None:
            changed = True
            removed += 1
            continue
        if new_line != line:
            changed = True
        out_lines.append(new_line)
    if changed:
        path.write_text("".join(out_lines), encoding="utf-8")
    return changed, removed


def main() -> int:
    repo_root = Path(__file__).resolve().parents[1]
    crates = repo_root / "crates"
    changed_files = 0
    removed_lines = 0

    for path in crates.rglob("*.rs"):
        did_change, removed = process_file(path)
        if did_change:
            changed_files += 1
            removed_lines += removed

    print(f"Changed files: {changed_files}")
    print(f"Removed comment lines: {removed_lines}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())

