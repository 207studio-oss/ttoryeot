#!/usr/bin/env python3
"""엔진과 플랫폼 모듈의 경계를 검사한다. 규칙은 ARCHITECTURE.md의 모듈 경계 절에 있다."""

import json
import re
import subprocess
import sys
from pathlib import Path

ENGINE = {"ttoryeot-hwp-core", "ttoryeot"}
SHARED = {"ttoryeot-alloc"}
FORBIDDEN_ENGINE_DEPS = {
    "libc", "windows", "windows-sys", "winapi", "objc", "objc2", "cocoa",
    "core-foundation", "core-text", "jni", "ndk", "nix", "rustix",
}
CFG = re.compile(r"\b(cfg_attr|cfg!?)\s*[\[(]")
SOURCE_BANS = [
    (re.compile(p), why)
    for p, why in [
        (r'extern\s+"', "C ABI"),
        (r"no_mangle", "내보내는 심볼"),
        (r"export_name", "내보내는 심볼"),
        (r"#\[\s*link\b", "네이티브 링크"),
        (r"global_allocator", "전역 할당자"),
        (r"std::os::", "운영체제 전용 API"),
        (r"std::process::(exit|Command)", "프로세스 제어"),
        (r"(std::)?env::var", "환경 변수"),
        (r"/System/Library|/usr/share|/Library/Fonts|WINDIR|C:\\\\Windows", "운영체제 경로"),
    ]
]


def cfg_arguments(text, start):
    """`cfg(`의 여는 괄호 위치에서 짝이 맞는 닫는 괄호까지의 내용을 돌려준다."""
    depth = 0
    for i in range(start, len(text)):
        if text[i] in "([":
            depth += 1
        elif text[i] in ")]":
            depth -= 1
            if depth == 0:
                return text[start + 1 : i]
    return text[start + 1 :]


def strip_comments(line):
    return "" if line.lstrip().startswith("//") else line


def scan_engine_sources(package, root, errors):
    manifest_dir = Path(package["manifest_path"]).parent
    for sub in ("src", "tests", "examples", "benches"):
        for path in sorted((manifest_dir / sub).rglob("*.rs")):
            text = "\n".join(strip_comments(l) for l in path.read_text().splitlines())
            where = path.relative_to(root)
            for match in CFG.finditer(text):
                argument = cfg_arguments(text, match.end() - 1).strip()
                allowed = argument == "test" or (
                    match.group(1) == "cfg_attr" and argument.split(",")[0].strip() == "test"
                )
                if not allowed:
                    line = text.count("\n", 0, match.start()) + 1
                    errors.append(f"{where}:{line}: 엔진은 cfg(test)만 쓴다. 찾은 것은 {match.group(0)}{argument})")
            if sub != "src":
                continue
            for number, line in enumerate(text.splitlines(), 1):
                for pattern, why in SOURCE_BANS:
                    if pattern.search(line):
                        errors.append(f"{where}:{number}: 엔진에 {why}를 두지 않는다. {line.strip()}")
    lib = manifest_dir / "src/lib.rs"
    if "#![forbid(unsafe_code)]" not in lib.read_text():
        errors.append(f"{lib.relative_to(root)}: 엔진은 #![forbid(unsafe_code)]를 선언한다")


def check_engine(package, root, errors):
    name = package["name"]
    for dep in package["dependencies"]:
        dep_name = dep["name"]
        if dep["target"]:
            errors.append(f"{name}: 대상별 의존을 두지 않는다. {dep_name} ({dep['target']})")
        if dep_name in FORBIDDEN_ENGINE_DEPS:
            errors.append(f"{name}: 플랫폼 크레이트에 직접 의존하지 않는다. {dep_name}")
        path = dep.get("path")
        if not path:
            continue
        if dep_name in ENGINE:
            continue
        if Path(path).resolve().is_relative_to(root / "vendor"):
            continue
        errors.append(f"{name}: 엔진은 플랫폼 모듈에 의존하지 않는다. {dep_name}")
    for target in package["targets"]:
        kinds = set(target["kind"])
        if kinds & {"bin", "custom-build"}:
            errors.append(f"{name}: 엔진에 실행 파일과 빌드 스크립트를 두지 않는다. {target['name']}")
        if kinds & {"staticlib", "cdylib"}:
            errors.append(f"{name}: 엔진은 네이티브 라이브러리를 만들지 않는다. {sorted(kinds)}")
    scan_engine_sources(package, root, errors)


def check_module(package, workspace, errors):
    name = package["name"]
    for dep in package["dependencies"]:
        dep_name = dep["name"]
        if dep_name in workspace and dep_name not in ENGINE | SHARED:
            errors.append(f"{name}: 플랫폼 모듈끼리 의존하지 않는다. {dep_name}")


def check_shared(package, workspace, errors):
    name = package["name"]
    for dep in package["dependencies"]:
        if dep["name"] in workspace:
            errors.append(f"{name}: 공용 크레이트는 작업공간에 의존하지 않는다. {dep['name']}")


def main():
    root = Path(sys.argv[1] if len(sys.argv) > 1 else Path(__file__).parent.parent).resolve()
    metadata = json.loads(
        subprocess.run(
            ["cargo", "metadata", "--no-deps", "--format-version", "1"],
            cwd=root, check=True, capture_output=True, text=True,
        ).stdout
    )
    packages = {p["name"]: p for p in metadata["packages"]}
    workspace = set(packages)
    errors = []
    for name, package in sorted(packages.items()):
        if name in ENGINE:
            check_engine(package, root, errors)
        elif name in SHARED:
            check_shared(package, workspace, errors)
        else:
            check_module(package, workspace, errors)
    for missing in sorted(ENGINE - workspace):
        errors.append(f"작업공간에 엔진 크레이트가 없다. {missing}")
    for error in errors:
        print(f"경계 위반 {error}", file=sys.stderr)
    if errors:
        return 1
    modules = sorted(workspace - ENGINE - SHARED)
    print(f"경계 정상. 엔진 {sorted(ENGINE)}, 플랫폼 {modules}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
