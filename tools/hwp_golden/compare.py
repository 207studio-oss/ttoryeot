#!/usr/bin/env python3
"""Compare reference and candidate text dumps; optionally gate on a stored baseline."""
import argparse
import json
import sys
from collections import Counter
from pathlib import Path

DEFAULT_BASELINE = Path(__file__).with_name("baseline_text.json")
DEFAULT_TEXT_ALLOW = Path(__file__).with_name("text_allow.txt")
GATED_METRICS = (
    "parse_ok",
    "parse_rate",
    "text_f1_all",
    "text_f1_parsed",
    "exact_text_docs",
    "table_count_match",
    "picture_count_match",
)


def load_jsonl(path):
    with open(path, encoding="utf-8") as handle:
        return {doc["file"]: doc for doc in map(json.loads, handle) if doc}


def load_text_allow(path):
    allowed = set()
    if not path:
        return allowed
    with open(path, encoding="utf-8") as handle:
        for raw in handle:
            line = raw.strip()
            if not line or line.startswith("#"):
                continue
            allowed.add(line.split(maxsplit=1)[0])
    return allowed


def text_f1(reference, candidate):
    left = Counter(ch for para in reference["paras"] for ch in para if not ch.isspace())
    right = Counter(ch for para in candidate["paras"] for ch in para if not ch.isspace())
    total = sum(left.values()) + sum(right.values())
    if total == 0:
        return 1.0
    return 2 * sum((left & right).values()) / total


def summarise(reference, candidate):
    files = [name for name, doc in reference.items() if doc.get("ok")]
    if not files:
        raise SystemExit("reference has no readable documents")

    per = {}
    parsed = 0
    f1_sum = 0.0
    table_matches = 0
    picture_matches = 0
    for name in files:
        ref = reference[name]
        cand = candidate.get(name, {"ok": False})
        if not cand.get("ok"):
            per[name] = [0.0, 0, 0]
            continue
        parsed += 1
        f1 = text_f1(ref, cand)
        table_match = ref["tables"] == cand["tables"]
        picture_match = ref["pictures"] == cand["pictures"]
        per[name] = [f1, int(table_match), int(picture_match)]
        f1_sum += f1
        table_matches += table_match
        picture_matches += picture_match

    count = len(files)
    summary = {
        "docs": count,
        "parse_ok": parsed,
        "parse_rate": round(parsed / count, 4),
        "text_f1_all": round(sum(value[0] for value in per.values()) / count, 4),
        "text_f1_parsed": round(f1_sum / max(parsed, 1), 4),
        "exact_text_docs": sum(value[0] > 0.9999 for value in per.values()),
        "table_count_match": round(table_matches / count, 4),
        "picture_count_match": round(picture_matches / count, 4),
    }
    return summary, per


def baseline_entry(data, key):
    if key:
        if key not in data:
            raise SystemExit(f"baseline has no key {key!r}")
        return key, data[key]
    if "rust" in data:
        return "rust", data["rust"]
    if "swift@05f647b7" in data:
        return "swift@05f647b7", data["swift@05f647b7"]
    if len(data) == 1:
        name = next(iter(data))
        return name, data[name]
    raise SystemExit("baseline key required: " + ", ".join(sorted(data)))


def allowed_exact(per, allowed):
    if per is None:
        return 0
    return sum(
        1
        for name in allowed
        if name in per and per[name] and per[name][0] > 0.9999
    )


def gate(summary, entry, key, tolerance, allowed=frozenset(), per=None):
    failures = []
    for metric in GATED_METRICS:
        expected = entry.get(metric)
        actual = summary.get(metric)
        if metric == "exact_text_docs" and "per" in entry:
            expected = expected - allowed_exact(entry["per"], allowed) if isinstance(expected, (int, float)) else expected
            actual = actual - allowed_exact(per, allowed) if isinstance(actual, (int, float)) else actual
        if not isinstance(expected, (int, float)) or not isinstance(actual, (int, float)):
            failures.append(f"{metric}: missing")
        elif actual + tolerance < expected:
            failures.append(f"{metric}: {actual} < {expected}")
    if failures:
        print(f"baseline {key}: FAIL " + "; ".join(failures), file=sys.stderr)
        return False
    print(f"baseline {key}: pass", file=sys.stderr)
    return True


def self_check():
    reference = {
        name: {"file": name, "ok": True, "tables": 0, "pictures": 0, "paras": ["abc"]}
        for name in ("a", "b", "c")
    }
    swift_candidate = {
        name: {"file": name, "ok": True, "tables": 0, "pictures": 0, "paras": ["abc"]}
        for name in ("a", "b", "c")
    }
    rust_candidate = {
        name: {
            "file": name,
            "ok": True,
            "tables": 0,
            "pictures": 0,
            "paras": ["abc" if name != "b" else "ab"],
        }
        for name in ("a", "b", "c")
    }
    allowed = {"b"}
    swift_summary, _ = summarise(reference, swift_candidate)
    rust_summary, rust_per = summarise(reference, rust_candidate)
    swift_baseline = {metric: swift_summary[metric] for metric in GATED_METRICS}
    rust_baseline = {metric: rust_summary[metric] for metric in GATED_METRICS}
    rust_baseline["exact_text_docs"] = 3
    rust_baseline["per"] = {name: [1.0, 0, 0] for name in reference}
    if not gate(swift_summary, swift_baseline, "self-swift", 0.0005, allowed):
        raise SystemExit("compare.py self-check: raw swift key failed")
    if not gate(rust_summary, rust_baseline, "self-rust", 0.0005, allowed, rust_per):
        raise SystemExit("compare.py self-check: adjusted rust key failed")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("reference", type=Path)
    parser.add_argument("candidate", type=Path)
    parser.add_argument(
        "--baseline",
        nargs="?",
        const=str(DEFAULT_BASELINE),
        metavar="JSON",
        help=f"gate against a baseline (default: {DEFAULT_BASELINE.name})",
    )
    parser.add_argument("--baseline-key")
    parser.add_argument("--per", type=Path, metavar="JSON")
    parser.add_argument(
        "--text-allow",
        type=Path,
        default=DEFAULT_TEXT_ALLOW,
        metavar="TXT",
        help="value-free reference-extractor exceptions (default: text_allow.txt)",
    )
    parser.add_argument("--tolerance", type=float, default=0.0005)
    args = parser.parse_args()

    self_check()
    reference = load_jsonl(args.reference)
    candidate = load_jsonl(args.candidate)
    allowed = load_text_allow(args.text_allow)
    summary, per = summarise(reference, candidate)
    print(json.dumps(summary, ensure_ascii=False))

    if args.per:
        with open(args.per, "w", encoding="utf-8") as handle:
            json.dump(per, handle, indent=0, sort_keys=True)
            handle.write("\n")

    if not args.baseline:
        return 0
    with open(args.baseline, encoding="utf-8") as handle:
        data = json.load(handle)
    key, entry = baseline_entry(data, args.baseline_key)
    return 0 if gate(summary, entry, key, args.tolerance, allowed, per) else 1


if __name__ == "__main__":
    sys.exit(main())
