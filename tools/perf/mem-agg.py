"""mem.sh 출력에서 이름·문서마다 중앙값[최소-최대]을 찍는다.

python3 mem-agg.py <out.jsonl>
phases 줄은 글꼴 몫(mb_fonts), 문서 몫(최대 - 글꼴), 최대, 전체 시간을,
mapped·read 줄은 부르기 전 앱 몫(mb_caller), 렌더러 몫(최대 - 앱), 최대, 전체 시간을 찍는다.
"""

import json
import statistics
import sys

rows = [json.loads(line) for line in open(sys.argv[1]) if '"phase":"run"' in line]
for row in rows:
    base = row["mb_fonts"] if row["kind"] == "phases" else row["mb_caller"]
    row["mb_base"] = base
    row["mb_doc"] = row["mb_peak"] - base
groups = dict.fromkeys((r["kind"], r["doc"], r["label"]) for r in rows)
for kind, doc, label in sorted(groups, key=lambda g: (g[0], g[1])):
    picked = [r for r in rows if (r["kind"], r["doc"], r["label"]) == (kind, doc, label)]
    cells = []
    for key in ("mb_base", "mb_doc", "mb_peak", "total"):
        values = sorted(r[key] for r in picked)
        cells.append(f"{key}={statistics.median(values):.1f}[{values[0]:.1f}-{values[-1]:.1f}]")
    print(f"{kind:7} {doc:16} {label:7} n={len(picked)} bytes={picked[0]['pdf_bytes']} " + " ".join(cells))
