#!/bin/zsh
# 대표 문서를 FFI처럼 쓰는 글꼴 파일만 넣어 새 프로세스로 잰다. 프로세스마다 3회차(0회차가 차가운 값).
# DOCS=<문서 폴더> FONTMAP=<TSV> tools/perf/cold.sh <out.jsonl> [반복 수] [phases 실행 파일]
# FONTMAP의 각 줄은 "문서 파일 이름<TAB>콜론으로 이은 글꼴 경로"이다. PDFS는 PDF를 쓸 폴더다.
B=${CARGO_TARGET_DIR:-target}/release/examples/phases
D=${DOCS:?문서 폴더}
map=${FONTMAP:?문서별 글꼴 TSV}
P=${PDFS:-/tmp/perf/cold}
out=${1:?out.jsonl}; reps=${2:-7}; bin=${3:-$B}
mkdir -p $P
uptime >> $out.uptime
for r in $(seq 1 $reps); do
  while IFS=$'\t' read -r d fonts; do
    $bin $D/$d "files:$fonts" 3 $P/${d%.*}.pdf | sed "s/^{/{\"doc\":\"$d\",\"rep\":$r,/" >> $out
  done < $map
done
uptime >> $out.uptime
