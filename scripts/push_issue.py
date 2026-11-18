#!/usr/bin/env python3
"""
push_issue.py — create the same issue in several Codeberg/Forgejo repositories.

Usage:
    export FORGEJO_TOKEN=...                       # token needs the write:issue scope
    ./push_issue.py "Title" "Body text"
    ./push_issue.py "Title" -f body.md             # body from a file
    echo "Body" | ./push_issue.py "Title" -f -     # body from stdin
    ./push_issue.py "Title" -e                     # write the body in $EDITOR
    ./push_issue.py "Title" -e -t TEMPLATE.md -s VERSION=0.3.6
                                                   # $EDITOR, prefilled from a template
    ./push_issue.py "Title" -t TEMPLATE.md -s VERSION=0.3.6 -s CHANGES=...
                                                   # template only, no editor
    ./push_issue.py "Title" "Body" --dry-run       # show what would happen
    ./push_issue.py "Title" "Body" -r pazufa/pazufa-backend   # pick only some repos
    ./push_issue.py "Title" "Body" -l Kind/Bug -l Priority/High  # override labels

As a library:
    from push_issue import push_issue
    push_issue("Title", "Body")
"""

from __future__ import annotations

import argparse
import json
import os
import re
import shlex
import subprocess
import sys
import tempfile
import urllib.error
import urllib.request
from dataclasses import dataclass

# ---------------------------------------------------------------------------
# Configuration
# ---------------------------------------------------------------------------

BASE_URL = "https://codeberg.org"

REPOS: list[str] = [
    "PaZuFa/pazufa-scraper-bb",
    "PaZuFa/pazufa-scraper-be",
    "PaZuFa/pazufa-scraper-by",
    "PaZuFa/pazufa-scraper-bw",
    "PaZuFa/pazufa-scraper-hb",
    "PaZuFa/pazufa-scraper-rp",
    "PaZuFa/pazufa-scraper-he",
    "PaZuFa/pazufa-scraper-sl",
    "PaZuFa/pazufa-scraper-sn",
    "PaZuFa/pazufa-scraper-st",
    "PaZuFa/pazufa-scraper-mv",
    "PaZuFa/pazufa-scraper-sh",
    "PaZuFa/pazufa-scraper-ni",
    "PaZuFa/pazufa-scraper-nw",
    "PaZuFa/pazufa-scraper-th",
    "PaZuFa/pazufa-scraper-hh",
    "PaZuFa/pazufa-scraper-bt",
    "PaZuFa/pazufa-scraper-core",
]

# Label names applied to every issue. Each must exist in the repo or its org.
LABELS: list[str] = [
    "notification",
]

TOKEN_ENV = "FORGEJO_TOKEN"

# ---------------------------------------------------------------------------


@dataclass
class Result:
    repo: str
    ok: bool
    url: str | None = None
    error: str | None = None


class ApiError(Exception):
    pass


def _api(method: str, path: str, token: str, base_url: str, payload: dict | None = None):
    req = urllib.request.Request(
        f"{base_url.rstrip('/')}/api/v1{path}",
        data=json.dumps(payload).encode() if payload is not None else None,
        method=method,
        headers={
            "Authorization": f"token {token}",
            "Content-Type": "application/json",
            "Accept": "application/json",
        },
    )
    try:
        with urllib.request.urlopen(req, timeout=30) as resp:
            return json.load(resp)
    except urllib.error.HTTPError as e:
        detail = e.read().decode(errors="replace")
        try:
            detail = json.loads(detail).get("message", detail)
        except (json.JSONDecodeError, AttributeError):
            pass
        raise ApiError(f"HTTP {e.code}: {detail}") from e
    except urllib.error.URLError as e:
        raise ApiError(str(e.reason)) from e


def _list_labels(path: str, token: str, base_url: str) -> list[dict]:
    labels, page = [], 1
    while True:
        batch = _api("GET", f"{path}?limit=50&page={page}", token, base_url)
        labels.extend(batch)
        if len(batch) < 50:
            return labels
        page += 1


_org_label_cache: dict[str, dict[str, int]] = {}


def _resolve_labels(repo: str, names: list[str], token: str, base_url: str) -> list[int]:
    """Map label names to IDs, using the repo's labels and its org's labels."""
    if not names:
        return []
    owner = repo.split("/", 1)[0]
    if owner not in _org_label_cache:
        try:
            org = _list_labels(f"/orgs/{owner}/labels", token, base_url)
        except ApiError:  # owner is a user, not an org
            org = []
        _org_label_cache[owner] = {l["name"]: l["id"] for l in org}
    available = dict(_org_label_cache[owner])
    available.update({l["name"]: l["id"] for l in _list_labels(f"/repos/{repo}/labels", token, base_url)})

    missing = [n for n in names if n not in available]
    if missing:
        raise ApiError(f"labels not found: {', '.join(missing)}")
    return [available[n] for n in names]


def _create_issue(repo: str, title: str, body: str, labels: list[str],
                  token: str, base_url: str, dry_run: bool) -> Result:
    try:
        label_ids = _resolve_labels(repo, labels, token, base_url)
        if dry_run:
            return Result(repo, True, url="(dry run)")
        data = _api("POST", f"/repos/{repo}/issues", token, base_url,
                    {"title": title, "body": body, "labels": label_ids})
        return Result(repo, True, url=data.get("html_url"))
    except ApiError as e:
        return Result(repo, False, error=str(e))


def push_issue(
    title: str,
    body: str = "",
    repos: list[str] | None = None,
    labels: list[str] | None = None,
    *,
    token: str | None = None,
    base_url: str = BASE_URL,
    dry_run: bool = False,
) -> list[Result]:
    """Create an issue with `title`/`body` in every repo. Returns one Result per repo."""
    if not title.strip():
        raise ValueError("title must not be empty")
    repos = repos if repos is not None else REPOS
    labels = labels if labels is not None else LABELS
    token = token or os.environ.get(TOKEN_ENV)
    if not token:
        raise RuntimeError(f"No token given and ${TOKEN_ENV} is not set")

    results = []
    for repo in repos:
        res = _create_issue(repo, title, body, labels, token, base_url, dry_run)
        results.append(res)
        mark = "✓" if res.ok else "✗"
        print(f"{mark} {repo}: {res.url if res.ok else res.error}", file=sys.stderr)
    return results


# Placeholders look like {NAME}: upper case, digits, underscores. Other braces
# (code blocks, JSON, ...) in the template are left alone.
_PLACEHOLDER = re.compile(r"\{([A-Z][A-Z0-9_]*)\}")


def render_template(text: str, values: dict[str, str]) -> str:
    """Replace {NAME} placeholders that have a value; leave the others in place."""
    return _PLACEHOLDER.sub(lambda m: values.get(m.group(1), m.group(0)), text)


def unfilled_placeholders(text: str) -> list[str]:
    return sorted(set(_PLACEHOLDER.findall(text)))


def _parse_set(items: list[str]) -> dict[str, str]:
    values = {}
    for item in items:
        key, sep, value = item.partition("=")
        if not sep or not _PLACEHOLDER.fullmatch("{" + key + "}"):
            raise ValueError(f"invalid --set {item!r}, expected NAME=value")
        values[key] = value.replace("\\n", "\n")
    return values


def _body_from_editor(initial: str = "") -> str:
    editor = os.environ.get("VISUAL") or os.environ.get("EDITOR") or "vi"
    with tempfile.NamedTemporaryFile("w+", suffix=".md", delete=False) as f:
        f.write(initial)
        path = f.name
    try:
        subprocess.run([*shlex.split(editor), path], check=True)
        with open(path) as f:
            return f.read()
    finally:
        os.unlink(path)


def main(argv: list[str] | None = None) -> int:
    p = argparse.ArgumentParser(description="Create the same issue in multiple Forgejo repos.")
    p.add_argument("title")
    src = p.add_mutually_exclusive_group()
    src.add_argument("body", nargs="?", default=None, help="issue body (Markdown)")
    src.add_argument("-f", "--body-file", help="read body from file ('-' for stdin)")
    src.add_argument("-e", "--edit", action="store_true", help="write body in $EDITOR")
    p.add_argument("-t", "--template",
                   help="body template with {NAME} placeholders; with -e it prefills the editor")
    p.add_argument("-s", "--set", action="append", default=[], metavar="NAME=VALUE",
                   help="fill a template placeholder (repeatable; '\\n' becomes a newline)")
    p.add_argument("-r", "--repo", action="append", dest="repos",
                   help="only these repos (repeatable); default: all configured")
    p.add_argument("-l", "--label", action="append", dest="labels",
                   help="use these labels instead of the configured ones (repeatable)")
    p.add_argument("--no-labels", action="store_true", help="create issues without labels")
    p.add_argument("-n", "--dry-run", action="store_true",
                   help="resolve labels but create nothing")
    p.add_argument("-y", "--yes", action="store_true", help="skip confirmation")
    p.add_argument("--base-url", default=BASE_URL)
    args = p.parse_args(argv)

    if args.template and (args.body is not None or args.body_file):
        p.error("-t/--template cannot be combined with a body or -f")
    if args.set and not args.template:
        p.error("-s/--set needs -t/--template")
    try:
        values = _parse_set(args.set)
    except ValueError as e:
        p.error(str(e))

    if args.template:
        with open(args.template) as f:
            body = render_template(f.read(), values)
        if args.edit:
            body = _body_from_editor(body)
        left = unfilled_placeholders(body)
        if left:
            print("error: unfilled placeholders: " + ", ".join("{%s}" % n for n in left)
                  + " (use -s NAME=value or -e)", file=sys.stderr)
            return 2
    elif args.body_file == "-":
        body = sys.stdin.read()
    elif args.body_file:
        with open(args.body_file) as f:
            body = f.read()
    elif args.edit:
        body = _body_from_editor()
    else:
        body = args.body or ""

    repos = args.repos or REPOS
    labels = [] if args.no_labels else (args.labels or LABELS)

    if not (args.yes or args.dry_run):
        print(f"Title: {args.title}\n\n{body}\n", file=sys.stderr)
        print("Labels: " + (", ".join(labels) or "(none)"), file=sys.stderr)
        print("Repos:\n  " + "\n  ".join(repos), file=sys.stderr)
        if input("Create issues? [y/N] ").strip().lower() != "y":
            print("Aborted.", file=sys.stderr)
            return 1

    try:
        results = push_issue(args.title, body, repos, labels,
                             base_url=args.base_url, dry_run=args.dry_run)
    except (ValueError, RuntimeError) as e:
        print(f"error: {e}", file=sys.stderr)
        return 2
    return 0 if all(r.ok for r in results) else 1


if __name__ == "__main__":
    sys.exit(main())
