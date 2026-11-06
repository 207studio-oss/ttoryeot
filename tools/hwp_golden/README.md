# 골든 기준 도구

`HWP_GOLDEN_DIR`을 지정한 골든 시험이 쓰는 파이썬 도구입니다. 말뭉치는 저장소에 없습니다.

| 파일 | 하는 일 |
|---|---|
| `reference.py` | HWP5·HWPX에서 문단·표·그림 수를 뽑아 JSON 줄로 씁니다. `olefile`이 필요합니다. |
| `compare.py` | 기준과 후보 덤프를 비교해 글자 F1·문서 수를 냅니다. |
| `baseline_text.json` | 비교에 쓰는 기준선입니다. |
| `text_allow.txt`, `fatal_allow.txt` | 알려진 차이를 허용하는 목록입니다. |

```bash
python3 -m pip install -r tools/hwp_golden/requirements.txt
HWP_GOLDEN_DIR=/path/to/documents cargo test -p ttoryeot-hwp-core golden -- --nocapture
```

`HWP_GOLDEN_DIR`이 없으면 골든 시험은 건너뜁니다. CI는 말뭉치를 쓰지 않습니다.
