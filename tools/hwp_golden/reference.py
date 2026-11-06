#!/usr/bin/env python3
"""Independent reference extractor: one JSON line per file {file, ok, tables, pictures, paras}."""
import json, re, struct, sys, zipfile, zlib, os
import olefile

EXT8 = set(range(1, 10)) | {11, 12} | set(range(14, 24))

def decode_para_text(b):
    u = struct.unpack('<%dH' % (len(b) // 2), b[:len(b) // 2 * 2])
    out, i = [], 0
    while i < len(u):
        c = u[i]
        if c < 32:
            if c == 9: out.append('\t')
            if c in EXT8: i += 8; continue
            i += 1; continue
        out.append(chr(c)); i += 1
    s = ''.join(out)
    try: return s.encode('utf-16', 'surrogatepass').decode('utf-16')
    except Exception: return s

def hwp5(path):
    ole = olefile.OleFileIO(path)
    hdr = ole.openstream('FileHeader').read()
    flags = struct.unpack('<I', hdr[36:40])[0]
    if flags & 2 or flags & 4: return None
    comp = flags & 1
    secs = sorted([e for e in ole.listdir() if len(e) == 2 and e[0] == 'BodyText'], key=lambda e: int(e[1][7:]))
    paras, tables, pics = [], 0, 0
    for e in secs:
        d = ole.openstream(e).read()
        if comp: d = zlib.decompress(d, -15)
        i = 0
        while i + 4 <= len(d):
            h = struct.unpack('<I', d[i:i+4])[0]; i += 4
            tag, size = h & 0x3ff, h >> 20
            if size == 0xfff: size = struct.unpack('<I', d[i:i+4])[0]; i += 4
            body = d[i:i+size]; i += size
            if tag == 66: paras.append('')
            elif tag == 67 and paras: paras[-1] = decode_para_text(body)
            elif tag == 71 and len(body) >= 4 and body[:4][::-1] == b'tbl ': tables += 1
            elif tag == 85: pics += 1
    return dict(tables=tables, pictures=pics, paras=paras)

def hwpx(path):
    z = zipfile.ZipFile(path)
    names = sorted([n for n in z.namelist() if re.match(r'Contents/section\d+\.xml$', n)], key=lambda n: int(re.findall(r'\d+', n)[-1]))
    from xml.etree import ElementTree as ET
    paras, tables, pics = [], 0, 0
    for n in names:
        root = ET.fromstring(z.read(n))
        def walk(el):
            nonlocal tables, pics
            loc = el.tag.split('}')[-1]
            if loc == 'tbl': tables += 1
            if loc == 'pic': pics += 1
            if loc == 'p':
                buf = []
                def txt(e):
                    l = e.tag.split('}')[-1]
                    if l == 't':
                        if e.text: buf.append(e.text)
                        for c in e:
                            if c.tag.split('}')[-1] == 'tab': buf.append('\t')
                            if c.tail: buf.append(c.tail)
                        return
                    if l in ('tbl', 'pic', 'container', 'rect', 'ellipse', 'drawText', 'subList', 'footNote', 'endNote', 'header', 'footer'):
                        walk(e); return
                    for c in e: txt(c)
                for c in el: txt(c)
                paras.append(''.join(buf)); return
            for c in el: walk(c)
        walk(root)
    return dict(tables=tables, pictures=pics, paras=paras)

for p in sys.argv[1:]:
    name = os.path.basename(p)
    try:
        r = hwpx(p) if p.lower().endswith('.hwpx') else hwp5(p)
    except Exception as e:
        r = None
    print(json.dumps({'file': name, 'ok': r is not None, **(r or {})}, ensure_ascii=False))
