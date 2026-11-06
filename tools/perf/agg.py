"""cold.sh 출력에서 문서마다 0회차(차가움)와 2회차(데움)의 중앙값[최소-최대]을 찍는다.

python3 agg.py <out.jsonl>
"""

import json
import statistics
import sys

KEYS = ["fonts", "parse", "layout", "pdf", "total", "mb_peak"]

rows = [json.loads(line) for line in open(sys.argv[1]) if '"phase":"run"' in line]
for doc in dict.fromkeys(r["doc"] for r in rows):
    for run, label in ((0, "cold"), (2, "warm")):
        picked = [r for r in rows if r["doc"] == doc and r["run"] == run]
        cells = []
        for key in KEYS:
            values = sorted(r[key] for r in picked)
            cells.append(f"{key}={statistics.median(values):.1f}[{values[0]:.1f}-{values[-1]:.1f}]")
        print(f"{doc:16} {label} p={picked[0]['pages']:2} n={len(picked)} " + " ".join(cells))
