"""macOS `sample` 출력의 호출 나무에서 함수별 포함 표본 수를 센다.

python3 sample-inclusive.py <sample.txt> [이름 일부 ...]
기다림(__ulock_wait 등) 표본은 뺀다. 같은 함수가 재귀로 겹치면 바깥 한 번만 센다.
"""

import re
import sys
from collections import Counter

LINE = re.compile(r"^(?P<lead>[\s+!:|]*)(?P<count>\d+) (?P<name>\S+)")
IDLE = ("__ulock_wait", "__psynch_cvwait", "__workq_kernreturn", "mach_msg2_trap", "__semwait_signal")


def short(name: str) -> str:
    parts = re.findall(r"\d+([A-Za-z_][A-Za-z0-9_]*)", name)
    return "::".join(parts[-3:]) if parts else name


def main() -> None:
    path, *needles = sys.argv[1:]
    text = open(path, encoding="utf-8", errors="replace").read()
    tree = text.split("Call graph:", 1)[1].split("Total number in stack", 1)[0]
    nodes = []
    for line in tree.splitlines():
        m = LINE.match(line)
        if m:
            nodes.append((len(m["lead"]), int(m["count"]), m["name"]))

    idle = Counter()
    for i, (depth, count, name) in enumerate(nodes):
        if name in IDLE:
            idle[i] = count

    inclusive = Counter()
    stack: list[tuple[int, str, int]] = []
    total = 0
    for i, (depth, count, name) in enumerate(nodes):
        while stack and stack[-1][0] >= depth:
            stack.pop()
        if not stack:
            total += count
        if name in IDLE:
            stack.append((depth, name, i))
            continue
        busy = count - sum(c for j, c in idle.items() if j > i and is_under(nodes, i, j))
        if all(short(n) != short(name) for _, n, _ in stack):
            inclusive[short(name)] += busy
        stack.append((depth, name, i))

    busy_total = total - sum(idle.values())
    print(f"busy samples {busy_total} (idle {sum(idle.values())})")
    rows = inclusive.most_common()
    if needles:
        rows = [r for r in rows if any(n in r[0] for n in needles)]
    for name, count in rows[:40]:
        print(f"{count:7d} {100 * count / busy_total:5.1f}%  {name}")


def is_under(nodes, parent: int, child: int) -> bool:
    depth = nodes[parent][0]
    for k in range(parent + 1, child + 1):
        if nodes[k][0] <= depth:
            return False
    return True


if __name__ == "__main__":
    main()
