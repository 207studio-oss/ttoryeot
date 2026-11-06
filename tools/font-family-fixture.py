#!/usr/bin/env python3
"""글꼴 이름마다 한 문단을 쓰는 합성 HWPX를 만든다. 계열 대체가 명조와 고딕을 가르는지 본다."""

import argparse
from pathlib import Path
from xml.sax.saxutils import quoteattr
from zipfile import ZIP_DEFLATED, ZIP_STORED, ZipFile, ZipInfo

FACES = ["바탕", "휴먼명조", "궁서", "돋움", "굴림", "맑은 고딕"]
LANGS = ["HANGUL", "LATIN", "HANJA", "JAPANESE", "OTHER", "SYMBOL", "USER"]

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("output", type=Path, help="생성할 HWPX 경로")
args = parser.parse_args()

fonts = "".join(f'<hh:font id="{index}" face={quoteattr(face)} type="TTF" isEmbedded="0"/>'
                for index, face in enumerate(FACES))
fontfaces = "".join(f'<hh:fontface lang="{lang}" fontCnt="{len(FACES)}">{fonts}</hh:fontface>'
                    for lang in LANGS)
refs = lambda index: " ".join(f'{lang.lower()}="{index}"' for lang in LANGS)
char_prs = "".join(f'<hh:charPr id="{index}" height="1200"><hh:fontRef {refs(index)}/></hh:charPr>'
                   for index in range(len(FACES)))
header = (
    '<hh:head xmlns:hh="http://www.hancom.co.kr/hwpml/2011/head" '
    'xmlns:hc="http://www.hancom.co.kr/hwpml/2011/core"><hh:refList>'
    f'<hh:fontfaces itemCnt="{len(LANGS)}">{fontfaces}</hh:fontfaces>'
    '<hh:paraProperties><hh:paraPr id="0"><hh:align horizontal="LEFT"/></hh:paraPr></hh:paraProperties>'
    f'<hh:charProperties itemCnt="{len(FACES)}">{char_prs}</hh:charProperties>'
    '</hh:refList></hh:head>'
).encode()
paragraphs = "".join(
    f'<hp:p paraPrIDRef="0"><hp:run charPrIDRef="{index}"><hp:t>{face} 가나다 ABC</hp:t></hp:run></hp:p>'
    for index, face in enumerate(FACES)
)
section = (
    '<hp:sec xmlns:hp="http://www.hancom.co.kr/hwpml/2011/paragraph" '
    f'xmlns:hc="http://www.hancom.co.kr/hwpml/2011/core">{paragraphs}</hp:sec>'
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
