#!/bin/zsh
# 대표 문서를 그 문서가 쓰는 글꼴 파일만 넣어 새 프로세스로 한 번씩 잰다.
# 여러 실행 파일을 같은 반복 안에서 번갈아 돌려 부하가 같은 때의 시간끼리 견준다.
# DOCS=<문서 폴더> FONTMAP=<TSV> tools/perf/mem.sh <out.jsonl> <반복 수> <이름=종류:실행 파일>...
#   FONTMAP의 각 줄은 "문서 파일 이름<TAB>콜론으로 이은 글꼴 경로"이다.
#   종류 phases는 ttoryeot-score의 phases 예제(글꼴을 읽어 넘김),
#   mapped, read는 ttoryeot-ffi의 ffi_mem 예제(앱이 글꼴을 매핑하거나 읽어 FFI로 넘김)이다.
D=${DOCS:?문서 폴더}
map=${FONTMAP:?문서별 글꼴 TSV}
P=${PDFS:-/tmp/perf/mem}
out=${1:?out.jsonl}; reps=${2:?reps}; shift 2
uptime >> $out.uptime
for r in $(seq 1 $reps); do
  while IFS=$'\t' read -r d fonts; do
    for spec in "$@"; do
      label=${spec%%=*}; rest=${spec#*=}; kind=${rest%%:*}; bin=${rest#*:}
      mkdir -p $P/$label; pdf=$P/$label/${d%.*}.pdf
      case $kind in
        phases) row=$($bin $D/$d "files:$fonts" 1 $pdf) ;;
        *) row=$($bin $D/$d "$fonts" $kind $pdf) ;;
      esac
      print -r -- "${row/#\{/{\"label\":\"$label\",\"kind\":\"$kind\",\"doc\":\"$d\",\"rep\":$r,}" >> $out
    done
  done < $map
done
uptime >> $out.uptime
