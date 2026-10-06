#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.12"
# dependencies = []
# ///

"""Serve the PR-gate aggregate statistics explorer on localhost."""

from __future__ import annotations

import argparse
import json
import sys
import threading
import webbrowser
from collections.abc import Sequence
from http import HTTPStatus
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path
from urllib.parse import urlsplit

SCRIPT_DIR = Path(__file__).resolve().parent
DEFAULT_DATA = SCRIPT_DIR / "aggstats.json"
DEFAULT_HTML = SCRIPT_DIR / "aggstats-explorer.html"


def validate_stats(path: Path) -> None:
    try:
        document = json.loads(path.read_text(encoding="utf-8"))
    except FileNotFoundError as error:
        raise ValueError(f"statistics file does not exist: {path}") from error
    except json.JSONDecodeError as error:
        raise ValueError(f"invalid JSON in {path}: {error}") from error

    if not isinstance(document, dict):
        raise TypeError("statistics document must be a JSON object")
    for key in ("groups", "tests"):
        if not isinstance(document.get(key), list):
            raise TypeError(f"statistics document must contain a '{key}' array")


def make_handler(html_path: Path, data_path: Path) -> type[BaseHTTPRequestHandler]:
    html = html_path.read_bytes()
    data = data_path.read_bytes()
    metadata = json.dumps({"filename": data_path.name}, ensure_ascii=False).encode()

    class ExplorerHandler(BaseHTTPRequestHandler):
        def do_GET(self) -> None:
            path = urlsplit(self.path).path
            if path in {"/", "/index.html"}:
                self.send_content(html, "text/html; charset=utf-8")
            elif path == "/data.json":
                self.send_content(data, "application/json; charset=utf-8")
            elif path == "/metadata.json":
                self.send_content(metadata, "application/json; charset=utf-8")
            else:
                self.send_error(HTTPStatus.NOT_FOUND)

        def send_content(self, content: bytes, content_type: str) -> None:
            self.send_response(HTTPStatus.OK)
            self.send_header("Content-Type", content_type)
            self.send_header("Content-Length", str(len(content)))
            self.send_header("Cache-Control", "no-store")
            self.send_header("X-Content-Type-Options", "nosniff")
            self.end_headers()
            self.wfile.write(content)

        def log_message(self, format: str, *args: object) -> None:
            return

    return ExplorerHandler


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "stats",
        nargs="?",
        type=Path,
        default=DEFAULT_DATA,
        help=f"aggregate JSON file (default: {DEFAULT_DATA.name})",
    )
    parser.add_argument(
        "--port", type=int, default=0, help="local port (default: automatic)"
    )
    parser.add_argument(
        "--no-open", action="store_true", help="do not open the system web browser"
    )
    return parser


def main(arguments: Sequence[str] | None = None) -> int:
    options = build_parser().parse_args(arguments)
    try:
        validate_stats(options.stats)
        handler = make_handler(DEFAULT_HTML, options.stats)
    except (OSError, TypeError, ValueError) as error:
        print(f"error: {error}", file=sys.stderr)
        return 1

    server = ThreadingHTTPServer(("127.0.0.1", options.port), handler)
    url = f"http://127.0.0.1:{server.server_port}/"
    print(f"Exploring {options.stats.resolve()}")
    print(f"Open {url} (Ctrl-C to stop)")

    if not options.no_open:
        threading.Timer(0.15, webbrowser.open, args=(url,)).start()

    try:
        server.serve_forever()
    except KeyboardInterrupt:
        print("\nStopped.")
    finally:
        server.server_close()
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
