#!/usr/bin/env python3
"""i18n CI checks: locale file integrity + bare translation-key detection.

Runs without any dependencies outside the Python stdlib (3.11+ for tomllib).

This is a *soft gate* — it always exits 0 so it never blocks a PR. Issues are
printed for visibility; they should be fixed over time. Once the codebase is
clean, flip the exit code to make this a hard gate.

Checks:
  1. zh-hans.toml keys that aren't in en.toml (stale/corrupt sections).
  2. Missing zh-hans translations (info only — English fallback is fine).
  3. tr() keys in .rs source that are missing from en.toml.
"""

import os
import re
import sys
import unicodedata
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parent.parent
EN_TOML = REPO_ROOT / "late-ssh" / "locales" / "en.toml"
ZH_HANS_TOML = REPO_ROOT / "late-ssh" / "locales" / "zh-hans.toml"
SRC_DIR = REPO_ROOT / "late-ssh" / "src"

# Keys consisting entirely of drawing / punctuation / whitespace characters.
# These are used in arcade UIs with tr() for layout purposes, not translation.
_DRAWING_CHARS = set(
    "─━│┃┄┅┆┇┈┉┊┋┌┍┎┏┐┑┒┓└┕┖┗┘┙┚┛├┝┞┟┠┡┢┣┤┥┦┧┨┩┪┫┬┭┮┯┰┱┲┳┴┵┶┷┸┹┺┻┼┽┾┿"
    "╀╁╂╃╄╅╆╇╈╉╊╋╌╍╎╏═║╒╓╔╕╖╗╘╙╚╛╜╝╞╟╠╡╢╣╤╥╦╧╨╩╪╫╬"
    "▀▁▂▃▄▅▆▇█▉▊▋▌▍▎▏▐░▒▓"
    " ◌…·●○◉◦"
)


def _is_drawing_key(key: str) -> bool:
    return all(c in _DRAWING_CHARS or c.isspace() for c in key)


def flatten_toml(raw: dict, prefix: str = "") -> set[str]:
    """Flatten nested TOML tables to dotted keys."""
    keys: set[str] = set()
    for k, v in raw.items():
        full = f"{prefix}{k}"
        if isinstance(v, dict):
            keys |= flatten_toml(v, f"{full}.")
        else:
            keys.add(full)
    return keys


def parse_toml_keys(path: Path) -> set[str]:
    """Parse a TOML file and return its flat key set."""
    if sys.version_info >= (3, 11):
        import tomllib
    else:
        try:
            import tomli as tomllib
        except ImportError:
            import subprocess
            subprocess.check_call(
                [sys.executable, "-m", "pip", "install", "-q", "tomli"]
            )
            import tomli as tomllib

    with open(path, "rb") as f:
        data = tomllib.load(f)
    return flatten_toml(data)


def check_locale_integrity() -> None:
    """Print locale file integrity warnings (always non-fatal)."""
    print("=== i18n locale integrity ===")
    en_keys = parse_toml_keys(EN_TOML)
    zh_keys = parse_toml_keys(ZH_HANS_TOML)

    extra_in_zh = zh_keys - en_keys
    if extra_in_zh:
        print(
            f"  warn: {len(extra_in_zh)} keys in zh-hans.toml"
            f" not present in en.toml (stale/corrupt sections)"
        )

    missing_in_zh = en_keys - zh_keys
    if missing_in_zh:
        print(
            f"  info: {len(missing_in_zh)} keys in en.toml"
            f" not yet translated to zh-hans"
        )
    print(f"  en keys: {len(en_keys)}  |  zh-hans keys: {len(zh_keys)}")


def find_tr_keys_in_rs(filepath: Path) -> list[tuple[int, str]]:
    """Extract (line_number, key) from tr(\"...\") / trf(\"...\", ...) calls."""
    results: list[tuple[int, str]] = []
    with open(filepath) as f:
        for lineno, line in enumerate(f, 1):
            # Word-boundary prevents matching push_str, get_str, etc.
            for m in re.finditer(r'(?<![_a-zA-Z])(?:i18n::)?tr(?:f)?\("([^"]+)"', line):
                results.append((lineno, m.group(1)))
    return results


def check_bare_keys() -> None:
    """Print tr() keys that are missing from en.toml."""
    print("\n=== bare / missing translation keys ===")
    en_keys = parse_toml_keys(EN_TOML)
    TEST_MARKERS = {"_test.rs", "/tests/", "/test_helpers.rs"}
    issues: list[str] = []

    for rs_file in sorted(SRC_DIR.rglob("*.rs")):
        rs_rel = rs_file.relative_to(REPO_ROOT)
        rs_str = str(rs_file)
        if any(m in rs_str for m in TEST_MARKERS):
            continue

        for lineno, key in find_tr_keys_in_rs(rs_file):
            if key not in en_keys and not _is_drawing_key(key):
                issues.append(
                    f"  {rs_rel}:{lineno}: tr(\"{key}\") — key not found in en.toml"
                )

    if issues:
        print(f"  warn: {len(issues)} key(s) missing from en.toml (soft gate)")
        for issue in issues:
            print(issue)
    else:
        print("  PASS: all tr() keys are present in en.toml")


def main() -> int:
    print("i18n check (soft gate — always passes)")
    check_locale_integrity()
    check_bare_keys()
    print("\nDone.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
