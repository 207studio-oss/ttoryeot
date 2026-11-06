#!/bin/zsh
# 두 렌더러 실행 파일로 채점 묶음의 모든 문서를 그려 PDF가 같은지 본다. 바이트가 다르면 쪽마다 화소를 견준다.
# SCORE=<채점 묶음 폴더> tools/perf/same-output.sh <before ttoryeot> <after ttoryeot> <out dir>
set -u
before=$1; after=$2; out=$3
score=${SCORE:?채점 묶음 폴더}
pdfpng=$score/tools/pdfpng
fonts=(--font "/Applications/Hancom Office HWP.app/Contents/Resources/Hnc/Shared/TTF" --font /System/Library/Fonts --font /Library/Fonts)
mkdir -p $out/b $out/a
same=0; pixels=0; differ=0
for doc in $score/{tuning,holdout,elements,elements-saved}/docs/*.hwp*(N); do
  stem=${${doc:h:h:t}}-${doc:t:r}
  $before render -q $doc -o $out/b/$stem.pdf $fonts 2>/dev/null
  $after render -q $doc -o $out/a/$stem.pdf $fonts 2>/dev/null
  if cmp -s $out/b/$stem.pdf $out/a/$stem.pdf; then same=$((same+1)); continue; fi
  $pdfpng $out/b/$stem.pdf $out/b/$stem 2 >/dev/null; $pdfpng $out/a/$stem.pdf $out/a/$stem 2 >/dev/null
  bad=0
  for png in $out/b/$stem-*.png(N); do cmp -s $png $out/a/${png:t} || bad=1; done
  [[ $(ls $out/b/$stem-*.png(N) | wc -l) == $(ls $out/a/$stem-*.png(N) | wc -l) ]] || bad=1
  if ((bad)); then differ=$((differ+1)); print "DIFFER $stem"; else pixels=$((pixels+1)); print "pixels-equal $stem"; fi
done
print "byte-equal=$same pixel-equal=$pixels differ=$differ"
