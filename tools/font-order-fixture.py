#!/usr/bin/env python3
"""글꼴 순서에 따른 줄 나눔을 재현할 합성 HWPX를 만든다."""

import argparse
from pathlib import Path
from zipfile import ZIP_DEFLATED, ZIP_STORED, ZipFile, ZipInfo

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("output", type=Path, help="생성할 HWPX 경로")
args = parser.parse_args()

fixture = Path(__file__).resolve().parents[1] / "hwp-core/tests/fixtures/synthetic.hwpx"
with ZipFile(fixture) as source:
    header = source.read("Contents/header.xml")

paragraphs = []
for index in range(1, 37):
    text = f"{index:02d} 가나다 글꼴 시험 ABCDEFG width test with spaces 문서 흐름 TEST."
    page = (
        '<hp:secPr><hp:pagePr width="19500" height="15000" '
        'landscape="WIDELY"><hp:margin left="1000" right="1000" '
        'top="1000" bottom="1000" header="0" footer="0" gutter="0"/>'
        '</hp:pagePr></hp:secPr>'
    ) if index == 1 else ""
    paragraphs.append(
        f'<hp:p paraPrIDRef="0"><hp:run charPrIDRef="0">{page}'
        f'<hp:t>{text}</hp:t></hp:run></hp:p>'
    )
section = (
    '<hp:sec xmlns:hp="http://www.hancom.co.kr/hwpml/2011/paragraph" '
    'xmlns:hc="http://www.hancom.co.kr/hwpml/2011/core">'
    + "".join(paragraphs)
    + "</hp:sec>"
).encode()

with ZipFile(args.output, "w") as output:
    for name, content in [
        ("mimetype", b"application/hwp+zip"),
        ("Contents/header.xml", header),
        ("Contents/section0.xml", section),
    ]:
        entry = ZipInfo(name, (2026, 1, 1, 0, 0, 0))
        entry.compress_type = ZIP_STORED if name == "mimetype" else ZIP_DEFLATED
        output.writestr(entry, content)
