"""시험지처럼 그림이 많은 HWPX를 만든다. 그림 하나가 들어 있는 HWPX를 본으로 쓴다.

python3 tools/perf/many-pictures.py <본.hwpx> <out.hwpx> <그림 수> <distinct|same|rgba|rgba-opaque|rgba-photo>
  distinct는 그림마다 다른 PNG(1200x800, 풀면 2.88 MB)를, same은 PNG 하나를 그림 수만큼 다시 쓴다.
  rgba는 distinct에 가장자리만 투명한 알파를 더하고, rgba-opaque는 알파가 전부 255다.
  rgba-photo는 rgba에 잡음을 섞어 말뭉치의 큰 RGBA PNG(풀면 2~4배)처럼 잘 눌리지 않는다.
  한 쪽에 그림 두 개씩 놓는다.
"""
import random
import re
import struct
import sys
import zipfile
import zlib

template, out, count, mode = sys.argv[1], sys.argv[2], int(sys.argv[3]), sys.argv[4]
src = zipfile.ZipFile(template)
section = src.read("Contents/section0.xml").decode()
hpf = src.read("Contents/content.hpf").decode()
picture = re.search(r"<hp:pic .*?</hp:pic>", section, re.S).group(0)
head = section[: section.find("<hp:p ")]
first = re.search(r"<hp:p .*?</hp:p>", section, re.S).group(0)
para_open = re.match(r"<hp:p [^>]*>", first).group(0)
run_open = re.search(r'<hp:run charPrIDRef="\d+">', section).group(0)


def png(seed, width=1200, height=800):
    # 잘 눌리는 그러데이션이라 파일은 작고 풀면 크다. 도표·수식 그림과 같은 꼴이다.
    rnd = random.Random(seed)
    a, b = rnd.randrange(1, 9), rnd.randrange(1, 9)
    noise = rnd.randbytes(width * 3 + 257) if mode == "rgba-photo" else bytes(width * 3 + 257)

    def color(x, y, c):
        # 행마다 잡음 자리를 옮겨, 앞 행을 그대로 베끼는 압축이 되지 않게 한다.
        wobble = noise[(x * 3 + c + y * 7) % len(noise)] & 31
        return ((x * a + y * b) // 4 + c * 80 + seed + wobble) & 255

    if mode.startswith("rgba"):
        def alpha(x, y):
            return 255 if mode == "rgba-opaque" or 20 <= x < width - 20 else 0
        rows = b"".join(
            b"\x00" + bytes(v for x in range(width) for v in (color(x, y, 0), color(x, y, 1), color(x, y, 2), alpha(x, y)))
            for y in range(height)
        )
    else:
        rows = b"".join(
            b"\x00" + bytes(color(x, y, c) for x in range(width) for c in range(3))
            for y in range(height)
        )

    def chunk(kind, data):
        return struct.pack(">I", len(data)) + kind + data + struct.pack(">I", zlib.crc32(kind + data))

    header = struct.pack(">IIBBBBB", width, height, 8, 6 if mode.startswith("rgba") else 2, 0, 0, 0)
    return b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", header) + chunk(b"IDAT", zlib.compress(rows, 9)) + chunk(b"IEND", b"")


refs = [f"image{i + 1}" if mode != "same" else "image1" for i in range(count)]
bins = sorted(set(refs), key=lambda ref: int(ref[5:]))
body = [first]
for i, ref in enumerate(refs):
    pic = re.sub(r'binaryItemIDRef="[^"]*"', f'binaryItemIDRef="{ref}"', picture)
    pic = re.sub(r' id="\d+"', f' id="{2000000 + i}"', pic, count=1)
    opened = re.sub(r' pageBreak="\d"', f' pageBreak="{int(i > 0 and i % 2 == 0)}"', para_open)
    body.append(f"{opened}{run_open}{pic}<hp:t>문항 {i + 1}</hp:t></hp:run></hp:p>")
hpf = re.sub(r'<opf:item id="image\d+"[^>]*/>', "", hpf)
items = "".join(f'<opf:item id="{ref}" href="BinData/{ref}.png" media-type="image/png" isEmbeded="1"/>' for ref in bins)
hpf = hpf.replace('<opf:item id="section0"', items + '<opf:item id="section0"', 1)
with zipfile.ZipFile(out, "w") as z:
    z.writestr(zipfile.ZipInfo("mimetype"), src.read("mimetype"), compress_type=zipfile.ZIP_STORED)
    for info in src.infolist():
        name = info.filename
        if name in ("mimetype", "Contents/section0.xml", "Contents/content.hpf") or name.startswith(("BinData/", "Preview/")):
            continue
        z.writestr(name, src.read(name), compress_type=zipfile.ZIP_DEFLATED)
    z.writestr("Contents/content.hpf", hpf, compress_type=zipfile.ZIP_DEFLATED)
    z.writestr("Contents/section0.xml", head + "".join(body) + "</hs:sec>", compress_type=zipfile.ZIP_DEFLATED)
    for i, ref in enumerate(bins):
        z.writestr(f"BinData/{ref}.png", png(i), compress_type=zipfile.ZIP_STORED)
print(out, count, mode, len(bins))
