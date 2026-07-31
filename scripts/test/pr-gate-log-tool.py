#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.12"
# dependencies = []
# ///

"""Download PR gate logs and aggregate cargo-nextest duration statistics.

Runtimes are reported in seconds. ``stdev`` and ``var`` are population
statistics over the observed test results, so both are 0.0 for one observation.
"""

from __future__ import annotations

import argparse
import json
import os
import re
import shutil
import statistics
import subprocess
import sys
import tempfile
from collections import defaultdict
from collections.abc import Iterable, Sequence
from dataclasses import dataclass, replace
from datetime import UTC, datetime, timedelta
from pathlib import Path
from typing import Any

DEFAULT_REPO = "mpiorowski/late-sh"
DEFAULT_WORKFLOW = "pr.yml"
DEFAULT_DAYS = 30
MAX_RUNS = 1_000
REPO_ROOT = Path(__file__).resolve().parents[2]
DEFAULT_LOGS_DIR = REPO_ROOT / "tmp" / "pr-gate-logs"
MANIFEST_NAME = ".pr-gate-log-manifest.json"

ANSI_ESCAPE_RE = re.compile(r"\x1b(?:\[[0-?]*[ -/]*[@-~]|\][^\x07]*(?:\x07|\x1b\\))")
GITHUB_TIMESTAMP_RE = re.compile(r"\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}(?:\.\d+)?Z\s+")
NEXTEST_RESULT_RE = re.compile(
    r"^(?P<status>"
    r"SLOW\s*\+\s*LEAK|FAIL\s*\+\s*LEAK|TIMEOUT-PASS|SLOW\+TMPASS|"
    r"FLAKY\s+\d+/\d+|FLKY-FL\s+\d+/\d+|"
    r"TRY\s+\d+\s+(?:PASS|FAIL|FL\+LK|LKFAIL|TMT|SIG[A-Z0-9]+|ABORT)|"
    r"LEAK-FAIL|XFAIL|TIMEOUT|SIG[A-Z0-9]+|ABORT|PASS|LEAK|SLOW|FAIL"
    r")\s+"
    r"\[\s*(?P<runtime>\d+(?:\.\d+)?)s\]\s+"
    r"\(\s*\d+/\d+\)\s+"
    r"(?P<group>\S+)\s+(?P<test>.+?)\s*$"
)


class ToolError(Exception):
    """An expected, user-actionable command failure."""


@dataclass(frozen=True)
class TestObservation:
    passed: bool
    runtime: float
    group: str
    test: str


def clean_log_line(line: str) -> str:
    """Remove ANSI codes and GitHub's optional job/step/timestamp prefix."""
    line = ANSI_ESCAPE_RE.sub("", line)
    timestamp = GITHUB_TIMESTAMP_RE.search(line)
    if timestamp is not None:
        line = line[timestamp.end() :]
    return line.strip()


def status_passed(status: str) -> bool:
    """Map nextest's human-readable final status to pass/fail."""
    status = " ".join(status.split())
    if status.startswith("TRY "):
        return status.endswith(" PASS")
    return status in {
        "PASS",
        "LEAK",
        "TIMEOUT-PASS",
        "SLOW",
        "SLOW + LEAK",
        "SLOW+TMPASS",
    } or status.startswith("FLAKY ")


def parse_nextest_results(lines: Iterable[str]) -> list[TestObservation]:
    """Extract one final observation per test from each nextest run in a log."""
    observations: list[TestObservation] = []
    current_run: dict[tuple[str, str], TestObservation] | None = None

    def finish_run() -> None:
        nonlocal current_run
        if current_run is not None:
            observations.extend(current_run.values())
            current_run = None

    for raw_line in lines:
        line = clean_log_line(raw_line)
        if "Nextest run ID" in line:
            finish_run()
            current_run = {}
            continue

        if current_run is None:
            continue

        match = NEXTEST_RESULT_RE.match(line)
        if match is not None:
            observation = TestObservation(
                passed=status_passed(match.group("status")),
                runtime=float(match.group("runtime")),
                group=match.group("group"),
                test=match.group("test"),
            )
            # A retried test can emit several TRY lines and then a FLAKY line.
            # Keeping the last record prevents attempts from inflating n.
            current_run[(observation.group, observation.test)] = observation
            continue

        if line.startswith("Summary ["):
            finish_run()

    # Cancelled jobs may end before nextest writes its summary.
    finish_run()
    return observations


def parse_log(path: Path) -> list[TestObservation]:
    with path.open(encoding="utf-8", errors="replace") as log_file:
        return parse_nextest_results(log_file)


def runtime_stats(observations: Sequence[TestObservation]) -> dict[str, float]:
    runtimes = [observation.runtime for observation in observations]
    return {
        "avg": statistics.fmean(runtimes),
        "min": min(runtimes),
        "max": max(runtimes),
        "median": statistics.median(runtimes),
        "stdev": statistics.pstdev(runtimes),
        "var": statistics.pvariance(runtimes),
    }


def merge_tests_with_same_basic_name(
    observations: Sequence[TestObservation],
) -> list[TestObservation]:
    """Merge aliases sharing a package and final ``::``-separated test name.

    Only identities with an actual collision are rewritten. This preserves the
    full nextest group and test names for observations that have no alias in the
    input data.
    """
    identities_by_basic_name: defaultdict[tuple[str, str], set[tuple[str, str]]] = (
        defaultdict(set)
    )
    for observation in observations:
        basic_group = observation.group.split("::", 1)[0]
        basic_test = observation.test.rsplit("::", 1)[-1]
        identities_by_basic_name[(basic_group, basic_test)].add(
            (observation.group, observation.test)
        )

    merged = []
    for observation in observations:
        basic_group = observation.group.split("::", 1)[0]
        basic_test = observation.test.rsplit("::", 1)[-1]
        identity_count = len(identities_by_basic_name[(basic_group, basic_test)])
        if identity_count > 1:
            observation = replace(
                observation,
                group=basic_group,
                test=basic_test,
            )
        merged.append(observation)
    return merged


def aggregate_stats(observations: Sequence[TestObservation]) -> dict[str, Any]:
    by_group: defaultdict[str, list[TestObservation]] = defaultdict(list)
    by_test: defaultdict[tuple[str, str], list[TestObservation]] = defaultdict(list)

    for observation in observations:
        by_group[observation.group].append(observation)
        by_test[(observation.group, observation.test)].append(observation)

    groups = []
    for name in sorted(by_group):
        group_observations = by_group[name]
        groups.append(
            {
                "name": name,
                "n": len(group_observations),
                "runtime": runtime_stats(group_observations),
                "passrate": sum(item.passed for item in group_observations)
                / len(group_observations),
            }
        )

    tests = []
    for group, name in sorted(by_test):
        test_observations = by_test[(group, name)]
        tests.append(
            {
                "name": name,
                "group": group,
                "n": len(test_observations),
                "runtime": runtime_stats(test_observations),
                "passrate": sum(item.passed for item in test_observations)
                / len(test_observations),
            }
        )

    return {"groups": groups, "tests": tests}


def gh_command() -> str:
    gh = shutil.which("gh")
    if gh is None:
        raise ToolError("gh is not on PATH")
    return gh


def run_gh_json(arguments: Sequence[str]) -> Any:
    command = [gh_command(), *arguments]
    result = subprocess.run(
        command,
        check=False,
        capture_output=True,
        text=True,
        encoding="utf-8",
        errors="replace",
    )
    if result.returncode != 0:
        detail = result.stderr.strip() or result.stdout.strip() or "unknown gh error"
        raise ToolError(f"{' '.join(command)} failed: {detail}")
    try:
        return json.loads(result.stdout)
    except json.JSONDecodeError as error:
        raise ToolError(f"gh returned invalid JSON: {error}") from error


def list_recent_pr_runs(repo: str) -> list[dict[str, Any]]:
    since = (datetime.now(UTC) - timedelta(days=DEFAULT_DAYS)).date().isoformat()
    runs = run_gh_json(
        [
            "run",
            "list",
            "--repo",
            repo,
            "--workflow",
            DEFAULT_WORKFLOW,
            "--event",
            "pull_request",
            "--created",
            f">={since}",
            "--limit",
            str(MAX_RUNS),
            "--json",
            "attempt,conclusion,createdAt,databaseId,status,url",
        ]
    )
    if not isinstance(runs, list):
        raise ToolError("gh run list returned an unexpected JSON value")
    if len(runs) == MAX_RUNS:
        print(
            f"warning: GitHub returned {MAX_RUNS} runs; the result may be truncated",
            file=sys.stderr,
        )
    return [
        run
        for run in runs
        if run.get("status") == "completed"
        and run.get("conclusion") not in {"action_required", "skipped"}
    ]


def log_filename(run_id: int, attempt: int) -> str:
    return f"run-{run_id}-attempt-{attempt}.log"


def download_run_attempt(
    *, repo: str, run_id: int, attempt: int, logs_dir: Path
) -> str:
    destination = logs_dir / log_filename(run_id, attempt)
    if destination.is_file() and destination.stat().st_size > 0:
        return "cached"

    descriptor, temporary_name = tempfile.mkstemp(
        prefix=f".{destination.name}.", suffix=".tmp", dir=logs_dir
    )
    temporary_path = Path(temporary_name)
    command = [
        gh_command(),
        "run",
        "view",
        str(run_id),
        "--repo",
        repo,
        "--attempt",
        str(attempt),
        "--log",
    ]

    try:
        with os.fdopen(descriptor, "w", encoding="utf-8") as output:
            result = subprocess.run(
                command,
                check=False,
                stdout=output,
                stderr=subprocess.PIPE,
                text=True,
                encoding="utf-8",
                errors="replace",
            )
        if result.returncode != 0:
            detail = result.stderr.strip() or "unknown gh error"
            raise ToolError(
                f"run {run_id} attempt {attempt} logs are unavailable: {detail}"
            )
        if temporary_path.stat().st_size == 0:
            raise ToolError(f"run {run_id} attempt {attempt} returned an empty log")
        temporary_path.replace(destination)
    except BaseException:
        temporary_path.unlink(missing_ok=True)
        raise

    return "downloaded"


def write_manifest(logs_dir: Path, repo: str, log_names: Sequence[str]) -> None:
    manifest = {
        "repo": repo,
        "workflow": DEFAULT_WORKFLOW,
        "window_days": DEFAULT_DAYS,
        "generated_at": datetime.now(UTC).isoformat(),
        "logs": list(log_names),
    }
    descriptor, temporary_name = tempfile.mkstemp(
        prefix=f".{MANIFEST_NAME}.", suffix=".tmp", dir=logs_dir
    )
    temporary_path = Path(temporary_name)
    try:
        with os.fdopen(descriptor, "w", encoding="utf-8") as output:
            json.dump(manifest, output, indent=2)
            output.write("\n")
        temporary_path.replace(logs_dir / MANIFEST_NAME)
    except BaseException:
        temporary_path.unlink(missing_ok=True)
        raise


def download_logs(repo: str, logs_dir: Path) -> None:
    logs_dir.mkdir(parents=True, exist_ok=True)
    runs = list_recent_pr_runs(repo)
    print(
        f"Found {len(runs)} completed PR runs from the last {DEFAULT_DAYS} days in {repo}.",
        file=sys.stderr,
    )

    counts = {"downloaded": 0, "cached": 0, "unavailable": 0}
    selected_logs = []
    for run in reversed(runs):
        run_id = int(run["databaseId"])
        latest_attempt = int(run.get("attempt", 1))
        for attempt in range(1, latest_attempt + 1):
            try:
                result = download_run_attempt(
                    repo=repo,
                    run_id=run_id,
                    attempt=attempt,
                    logs_dir=logs_dir,
                )
            except ToolError as error:
                counts["unavailable"] += 1
                print(f"warning: {error}", file=sys.stderr)
            else:
                counts[result] += 1
                selected_logs.append(log_filename(run_id, attempt))

    write_manifest(logs_dir, repo, selected_logs)

    print(
        "PR logs: "
        f"{counts['downloaded']} downloaded, "
        f"{counts['cached']} cached, "
        f"{counts['unavailable']} unavailable; "
        f"directory: {logs_dir}",
        file=sys.stderr,
    )


def log_paths(logs_dir: Path) -> list[Path]:
    if not logs_dir.is_dir():
        raise ToolError(f"logs directory does not exist: {logs_dir}")

    manifest_path = logs_dir / MANIFEST_NAME
    if manifest_path.is_file():
        try:
            manifest = json.loads(manifest_path.read_text(encoding="utf-8"))
        except json.JSONDecodeError as error:
            raise ToolError(
                f"invalid download manifest {manifest_path}: {error}"
            ) from error
        names = manifest.get("logs") if isinstance(manifest, dict) else None
        if not isinstance(names, list) or not all(
            isinstance(name, str) for name in names
        ):
            raise ToolError(
                f"invalid download manifest {manifest_path}: logs must be a list"
            )
        if any(Path(name).name != name for name in names):
            raise ToolError(
                f"invalid download manifest {manifest_path}: unsafe log name"
            )
        paths = [logs_dir / name for name in names]
        missing = [path for path in paths if not path.is_file()]
        if missing:
            raise ToolError(f"download manifest references missing log: {missing[0]}")
        return paths

    return sorted(
        path
        for path in logs_dir.rglob("*")
        if path.is_file() and path.suffix in {".log", ".txt"}
    )


def aggregate_logs(
    logs_dir: Path, *, merge_same_basic_name: bool = False
) -> dict[str, Any]:
    paths = log_paths(logs_dir)
    if not paths:
        raise ToolError(f"no .log or .txt files found under {logs_dir}")

    observations = [observation for path in paths for observation in parse_log(path)]
    if not observations:
        raise ToolError(f"no nextest result records found in {len(paths)} log files")

    if merge_same_basic_name:
        observations = merge_tests_with_same_basic_name(observations)

    print(
        f"Aggregated {len(observations)} test observations from {len(paths)} log files.",
        file=sys.stderr,
    )
    return aggregate_stats(observations)


def write_stats_file(path: Path, stats: dict[str, Any]) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    descriptor, temporary_name = tempfile.mkstemp(
        prefix=f".{path.name}.", suffix=".tmp", dir=path.parent
    )
    temporary_path = Path(temporary_name)
    try:
        with os.fdopen(descriptor, "w", encoding="utf-8") as output:
            json.dump(stats, output, indent=2)
            output.write("\n")
        temporary_path.replace(path)
    except BaseException:
        temporary_path.unlink(missing_ok=True)
        raise


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--dl-logs",
        action="store_true",
        help=f"download completed {DEFAULT_WORKFLOW} run logs from the last {DEFAULT_DAYS} days",
    )
    parser.add_argument(
        "--output-stats-file",
        type=Path,
        metavar="JSONFILE",
        help="write aggregate nextest runtime and pass-rate statistics to JSONFILE",
    )
    parser.add_argument(
        "--merge-tests-with-same-basic-name",
        action="store_true",
        help=(
            "merge raw observations with the same package and final "
            ":: separated test name before calculating statistics"
        ),
    )
    parser.add_argument(
        "--logs-dir",
        type=Path,
        default=DEFAULT_LOGS_DIR,
        help=f"download/input directory (default: {DEFAULT_LOGS_DIR})",
    )
    parser.add_argument(
        "--repo",
        default=DEFAULT_REPO,
        help=f"GitHub OWNER/REPO containing the PR workflow (default: {DEFAULT_REPO})",
    )
    return parser


def main(arguments: Sequence[str] | None = None) -> int:
    parser = build_parser()
    options = parser.parse_args(arguments)
    if options.merge_tests_with_same_basic_name and options.output_stats_file is None:
        parser.error("--merge-tests-with-same-basic-name requires --output-stats-file")
    if not options.dl_logs and options.output_stats_file is None:
        parser.error("select --dl-logs and/or --output-stats-file JSONFILE")

    try:
        if options.dl_logs:
            download_logs(options.repo, options.logs_dir)
        if options.output_stats_file is not None:
            stats = aggregate_logs(
                options.logs_dir,
                merge_same_basic_name=options.merge_tests_with_same_basic_name,
            )
            write_stats_file(options.output_stats_file, stats)
            print(
                f"Wrote aggregate statistics to {options.output_stats_file}.",
                file=sys.stderr,
            )
    except (OSError, ToolError) as error:
        print(f"error: {error}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
