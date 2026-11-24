#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.11"
# dependencies = ["rich>=13"]
# ///
"""
auth_tool.py — manage PaZuFa API keys: status, list, inspect, create, revoke, rotate.

Run without a command for an interactive menu:
    ./auth_tool.py -b staging

Or use subcommands (good for scripts; a newly created key goes to stdout):
    ./auth_tool.py -b prod status
    ./auth_tool.py -b prod list
    ./auth_tool.py -b prod show scraperBY4kd8sLq0
    ./auth_tool.py -b prod create collector --expires 90d --prefix scraperBY
    ./auth_tool.py -b prod revoke scraperBY4kd8sLq0
    ./auth_tool.py -b prod rotate

The key you authenticate with comes from $PAZUFA_API_KEY, --key-file, or a hidden
prompt. It is deliberately not accepted as an argument, so it never ends up in shell
history or `ps`. -b also reads $PAZUFA_BACKEND.

Scopes: create, revoke, list and show need a `keyadder` key. Any key can see its own
status and rotate itself (rotation only ever rotates the key you authenticate with).
"""

from __future__ import annotations

import argparse
import datetime as dt
import getpass
import json
import os
import re
import sys
import urllib.error
import urllib.parse
import urllib.request
from dataclasses import dataclass

from rich import box
from rich.console import Console
from rich.panel import Panel
from rich.prompt import Confirm, Prompt
from rich.table import Table

BACKENDS = {
    "prod": "https://api.pazufa.de",
    "staging": "https://staging.api.pazufa.de",
    "local": "http://localhost:8080",
}
SCOPES = ["collector", "admin", "keyadder"]
KEY_ENV = "PAZUFA_API_KEY"
BACKEND_ENV = "PAZUFA_BACKEND"

# Mirrors pazufa-models/src/auth.rs
KEY_LENGTH = 128
KEYTAG_LENGTH = 16
KEYTAG_MAX_PREFIX_LENGTH = 10
KEY_PREFIX = "pazufa-"
CRC_LENGTH = 8

console = Console(stderr=True, highlight=False)


# ---------------------------------------------------------------------------
# Key format helpers
# ---------------------------------------------------------------------------

def _crc32_cksum(data: bytes) -> int:
    """CRC-32/CKSUM (crc crate's CRC_32_CKSUM): poly 0x04C11DB7, init 0, no reflection, xorout ~0."""
    crc = 0
    for byte in data:
        crc ^= byte << 24
        for _ in range(8):
            crc = ((crc << 1) ^ 0x04C11DB7) if crc & 0x80000000 else (crc << 1)
            crc &= 0xFFFFFFFF
    return crc ^ 0xFFFFFFFF


def key_problem(key: str) -> str | None:
    """Return why `key` is not a well-formed API key, or None if it looks fine."""
    if not key.startswith(KEY_PREFIX):
        return f"does not start with '{KEY_PREFIX}'"
    if len(key) != KEY_LENGTH:
        return f"has {len(key)} characters, expected {KEY_LENGTH}"
    body, crc = key[:-CRC_LENGTH], key[-CRC_LENGTH:]
    if f"{_crc32_cksum(body.encode()):08x}" != crc.lower():
        return "checksum does not match (typo or truncated copy?)"
    return None


def keytag_of(key: str) -> str:
    return key[len(KEY_PREFIX):len(KEY_PREFIX) + KEYTAG_LENGTH]


def parse_expiry(text: str) -> dt.datetime | None:
    """'' -> None (server default: 365 days). Accepts 90d / 12h / 2w / 30m or an ISO date(time).
    Naive datetimes are taken as local time."""
    text = text.strip()
    if not text:
        return None
    now = dt.datetime.now(dt.timezone.utc)
    m = re.fullmatch(r"(\d+)\s*([mhdw])", text, re.IGNORECASE)
    if m:
        n, unit = int(m.group(1)), m.group(2).lower()
        delta = {"m": dt.timedelta(minutes=n), "h": dt.timedelta(hours=n),
                 "d": dt.timedelta(days=n), "w": dt.timedelta(weeks=n)}[unit]
        when = now + delta
    else:
        try:
            when = dt.datetime.fromisoformat(text)
        except ValueError:
            raise ValueError(f"cannot parse expiry {text!r} (try 90d, 12h, 2027-01-31 or 2027-01-31T18:00)")
        if when.tzinfo is None:
            when = when.astimezone()  # interpret as local time
        when = when.astimezone(dt.timezone.utc)
    if when <= now:
        raise ValueError("expiry lies in the past")
    return when.replace(microsecond=0)


def fmt_time(value: str | dt.datetime) -> str:
    when = dt.datetime.fromisoformat(value) if isinstance(value, str) else value
    local = when.astimezone()
    delta = when - dt.datetime.now(dt.timezone.utc)
    secs = delta.total_seconds()
    if secs < 0:
        rel = "expired"
    elif secs < 3600:
        rel = f"in {int(secs // 60)} min"
    elif secs < 2 * 86400:
        rel = f"in {secs / 3600:.0f} h"
    else:
        rel = f"in {delta.days} days"
    return f"{local:%Y-%m-%d %H:%M %Z} ({rel})"


# ---------------------------------------------------------------------------
# API client
# ---------------------------------------------------------------------------

class ApiError(Exception):
    def __init__(self, status: int | None, message: str):
        self.status = status
        super().__init__(f"HTTP {status}: {message}" if status else message)


@dataclass
class Response:
    status: int
    headers: dict[str, str]
    body: str

    def json(self):
        return json.loads(self.body) if self.body else None


class Client:
    def __init__(self, base_url: str, key: str):
        self.base_url = base_url.rstrip("/")
        self.key = key

    @property
    def keytag(self) -> str:
        return keytag_of(self.key)

    def _request(self, method: str, path: str, *, headers: dict | None = None,
                 payload: dict | None = None) -> Response:
        hdrs = {"x-api-key": self.key, "Accept": "application/json",
                "User-Agent": "pazufa-auth-tool"}
        hdrs.update(headers or {})
        data = None
        if payload is not None:
            data = json.dumps(payload).encode()
            hdrs["Content-Type"] = "application/json"
        req = urllib.request.Request(f"{self.base_url}/api/v2/auth{path}",
                                     data=data, method=method, headers=hdrs)
        try:
            with urllib.request.urlopen(req, timeout=30) as resp:
                return Response(resp.status, {k.lower(): v for k, v in resp.headers.items()},
                                resp.read().decode(errors="replace"))
        except urllib.error.HTTPError as e:
            detail = e.read().decode(errors="replace").strip()
            hint = {401: "key invalid, expired, revoked, or lacks the required scope",
                    403: "key lacks the required scope"}.get(e.code, "")
            raise ApiError(e.code, detail or hint or e.reason) from e
        except urllib.error.URLError as e:
            raise ApiError(None, f"cannot reach {self.base_url}: {e.reason}") from e

    def status(self) -> dict:
        return self._request("GET", "/status").json()

    def list_keytags(self) -> list[str]:
        tags, page = [], 1
        while True:
            resp = self._request("GET", f"/keys?page={page}&per_page=100")
            if resp.status == 204:
                return tags
            tags.extend(resp.json() or [])
            total_pages = int(resp.headers.get("x-total-pages", page))
            if page >= total_pages:
                return tags
            page += 1

    def touched_by(self, keytag: str) -> dict[str, list[str]]:
        return self._request("GET", f"/keys/{urllib.parse.quote(keytag, safe='')}").json() or {}

    def create(self, scope: str, expires_at: dt.datetime | None, prefix: str | None) -> str:
        payload: dict = {"scope": scope}
        if expires_at:
            payload["expires_at"] = expires_at.isoformat().replace("+00:00", "Z")
        if prefix:
            payload["keytag_prefix"] = prefix
        return self._request("POST", "", payload=payload).body.strip()

    def revoke(self, keytag: str) -> None:
        self._request("DELETE", "", headers={"api-key-delete": keytag})

    def rotate(self) -> dict:
        return self._request("POST", "/rotate").json()


# ---------------------------------------------------------------------------
# UI pieces
# ---------------------------------------------------------------------------

@dataclass
class Session:
    client: Client
    backend_name: str
    scope: str
    assume_yes: bool
    interactive: bool

    @property
    def is_prod(self) -> bool:
        return self.client.base_url == BACKENDS["prod"]

    def confirm(self, question: str) -> bool:
        if self.assume_yes:
            return True
        if not sys.stdin.isatty():
            raise SystemExit("error: confirmation needed but stdin is not a terminal (use -y)")
        if self.is_prod:
            question = f"[bold red]PROD[/] · {question}"
        return Confirm.ask(question, console=console, default=False)

    def require_keyadder(self, action: str) -> bool:
        if self.scope != "keyadder":
            console.print(f"[red]✗[/] {action} needs a [bold]keyadder[/] key; "
                          f"you are logged in as [bold]{self.scope}[/].")
            if not self.interactive:
                raise SystemExit(1)
            return False
        return True


def backend_label(name: str, url: str) -> str:
    color = "red" if url == BACKENDS["prod"] else "yellow" if name == "staging" else "cyan"
    return f"[bold {color}]{name}[/] [dim]{url}[/]"


def show_status(s: Session, st: dict | None = None) -> None:
    st = st or s.client.status()
    t = Table.grid(padding=(0, 2))
    t.add_column(style="dim")
    t.add_column()
    t.add_row("Backend", backend_label(s.backend_name, s.client.base_url))
    t.add_row("Keytag", f"[bold]{s.client.keytag}[/]")
    t.add_row("Scope", st["scope"])
    t.add_row("Expires", fmt_time(st["expires_at"]))
    if st.get("is_being_rotated"):
        t.add_row("Rotation", "[yellow]in progress[/] — this key stops working at the expiry above")
    console.print(Panel(t, title="Logged in", box=box.ROUNDED, expand=False))


def emit_key(s: Session, key: str, title: str, extra: str = "") -> None:
    """Show a freshly minted key. In script mode with stdout redirected, print just the key."""
    if not s.interactive and not sys.stdout.isatty():
        print(key)
        console.print(f"[green]✓[/] {title}: keytag [bold]{keytag_of(key)}[/] (key written to stdout)")
        return
    body = f"[bold green]{key}[/]\n\n[dim]keytag[/] {keytag_of(key)}"
    if extra:
        body += f"\n{extra}"
    body += "\n\n[yellow]This is the only time the key is shown.[/]"
    console.print(Panel(body, title=title, box=box.HEAVY, expand=False))
    if s.interactive and Confirm.ask("Save it to a file?", console=console, default=False):
        save_key(key)


def save_key(key: str) -> None:
    path = os.path.expanduser(Prompt.ask("Path", console=console, default=f"./{keytag_of(key)}.key"))
    try:
        fd = os.open(path, os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o600)
    except FileExistsError:
        console.print(f"[red]✗[/] {path} exists, not overwriting.")
        return
    with os.fdopen(fd, "w") as f:
        f.write(key + "\n")
    console.print(f"[green]✓[/] saved to {path} (mode 600)")


# ---------------------------------------------------------------------------
# Actions
# ---------------------------------------------------------------------------

def act_list(s: Session) -> list[str]:
    if not s.require_keyadder("Listing keys"):
        return []
    with console.status("Fetching keys…"):
        tags = s.client.list_keytags()
    t = Table(title=f"Active keys ({len(tags)})", box=box.SIMPLE_HEAD)
    t.add_column("#", justify="right", style="dim")
    t.add_column("Keytag")
    for i, tag in enumerate(tags, 1):
        t.add_row(str(i), f"[bold]{tag}[/]  [cyan](you)[/]" if tag == s.client.keytag else tag)
    console.print(t)
    console.print("[dim]Newest expiry first. Scope and expiry of other keys are not exposed by the API.[/]")
    return tags


def act_show(s: Session, keytag: str) -> None:
    if not s.require_keyadder("Inspecting keys"):
        return
    with console.status(f"Fetching objects touched by {keytag}…"):
        touched = s.client.touched_by(keytag)
    t = Table(title=f"Objects written by {keytag}", box=box.SIMPLE_HEAD)
    t.add_column("Kind")
    t.add_column("Count", justify="right")
    t.add_column("Examples", style="dim", overflow="fold")
    for kind in ("vorgaenge", "stationen", "sitzungen", "dokumente"):
        ids = touched.get(kind) or []
        t.add_row(kind, str(len(ids)), ", ".join(ids[:3]) + (" …" if len(ids) > 3 else ""))
    console.print(t)


def act_create(s: Session, scope: str | None = None, expiry: str | None = None,
               prefix: str | None = None) -> None:
    if not s.require_keyadder("Creating keys"):
        return
    if scope is None:
        scope = Prompt.ask("Scope", choices=SCOPES, default="collector", console=console)
    while True:
        if expiry is None:
            expiry = Prompt.ask("Expires [dim](90d, 12h, 2027-01-31, empty = server default 365d)[/]",
                                default="", show_default=False, console=console)
        try:
            expires_at = parse_expiry(expiry)
            break
        except ValueError as e:
            if not s.interactive:
                raise SystemExit(f"error: {e}")
            console.print(f"[red]✗[/] {e}")
            expiry = None
    while True:
        if prefix is None:
            prefix = Prompt.ask(f"Keytag prefix [dim](≤{KEYTAG_MAX_PREFIX_LENGTH} chars, empty = random)[/]",
                                default="", show_default=False, console=console)
        problem = None
        if len(prefix) > KEYTAG_MAX_PREFIX_LENGTH:
            problem = f"prefix is {len(prefix)} chars; the server would cut it to {prefix[:KEYTAG_MAX_PREFIX_LENGTH]!r}"
        elif prefix and not re.fullmatch(r"[A-Za-z0-9_-]+", prefix):
            problem = "prefix should only contain letters, digits, '_' and '-'"
        if not problem:
            break
        if not s.interactive:
            raise SystemExit(f"error: {problem}")
        console.print(f"[red]✗[/] {problem}")
        prefix = None

    summary = Table.grid(padding=(0, 2))
    summary.add_column(style="dim")
    summary.add_column()
    summary.add_row("Backend", backend_label(s.backend_name, s.client.base_url))
    summary.add_row("Scope", f"[bold]{scope}[/]")
    summary.add_row("Expires", fmt_time(expires_at) if expires_at else "server default (365 days)")
    summary.add_row("Prefix", prefix or "[dim](random keytag)[/]")
    console.print(Panel(summary, title="New key", expand=False))
    if not s.confirm("Create this key?"):
        console.print("Aborted.")
        return
    with console.status("Creating…"):
        key = s.client.create(scope, expires_at, prefix or None)
    emit_key(s, key, f"Created {scope} key")


def act_revoke(s: Session, keytags: list[str] | None = None) -> None:
    if not s.require_keyadder("Revoking keys"):
        return
    with console.status("Fetching keys…"):
        active = s.client.list_keytags()
    if not keytags:
        tags = act_list(s) if s.interactive else active
        if not tags:
            return
        raw = Prompt.ask("Keytag(s) or list numbers to revoke [dim](space separated)[/]", console=console)
        keytags = []
        for item in raw.split():
            if item.isdigit() and 1 <= int(item) <= len(tags):
                keytags.append(tags[int(item) - 1])
            else:
                keytags.append(item)
    if not keytags:
        return

    unknown = [k for k in keytags if k not in active]
    if unknown:
        # The server answers 204 for unknown keytags too, so check here.
        console.print("[red]✗[/] not an active key: " + ", ".join(unknown))
        return
    for tag in keytags:
        if tag == s.client.keytag:
            console.print(f"[yellow]![/] {tag} is the key you are logged in with — it stops working immediately.")
    for tag in keytags:
        act_show(s, tag)
    if not s.confirm(f"Revoke {len(keytags)} key(s): {', '.join(keytags)}?"):
        console.print("Aborted.")
        return
    for tag in keytags:
        try:
            s.client.revoke(tag)
            console.print(f"[green]✓[/] revoked {tag}")
        except ApiError as e:
            console.print(f"[red]✗[/] {tag}: {e}")


def act_rotate(s: Session) -> None:
    target = s.client
    if s.interactive:
        choice = Prompt.ask("Rotate which key? [dim](the API can only rotate the key making the call)[/]",
                            choices=["mine", "other"], default="mine", console=console)
        if choice == "other":
            target = Client(s.client.base_url, read_key_interactively("Key to rotate"))
    with console.status("Checking key…"):
        st = target.status()
    console.print(f"Rotating [bold]{target.keytag}[/] ({st['scope']}, expires {fmt_time(st['expires_at'])}).")
    console.print("[dim]The new key gets the same scope and lifetime. The old key stays valid for at most "
                  "one more day. Rotating twice in a row invalidates the first replacement.[/]")
    if st.get("is_being_rotated"):
        console.print("[yellow]![/] This key is already being rotated; the key that replaced it will be invalidated.")
    if not s.confirm("Rotate?"):
        console.print("Aborted.")
        return
    with console.status("Rotating…"):
        res = target.rotate()
    new_key = res["new_api_key"]
    emit_key(s, new_key, "Rotated key",
             f"[dim]old key[/] {target.keytag} stops working {fmt_time(res['rotation_complete_date'])}")
    if s.interactive and target is s.client and \
            Confirm.ask("Continue this session with the new key?", console=console, default=True):
        s.client.key = new_key


# ---------------------------------------------------------------------------
# Startup
# ---------------------------------------------------------------------------

def read_key_interactively(label: str = "API key") -> str:
    while True:
        key = getpass.getpass(f"{label} (hidden): ").strip()
        problem = key_problem(key)
        if not problem:
            return key
        console.print(f"[red]✗[/] key {problem}")


def resolve_key(key_file: str | None) -> str:
    if key_file:
        with open(os.path.expanduser(key_file)) as f:
            key = f.read().strip()
        source = key_file
    elif os.environ.get(KEY_ENV):
        key, source = os.environ[KEY_ENV].strip(), f"${KEY_ENV}"
    elif sys.stdin.isatty():
        return read_key_interactively()
    else:
        raise SystemExit(f"error: no key; set ${KEY_ENV}, pass --key-file, or run in a terminal")
    problem = key_problem(key)
    if problem:
        raise SystemExit(f"error: key from {source} {problem}")
    return key


def resolve_backend(value: str | None) -> tuple[str, str]:
    if value is None and sys.stdin.isatty():
        value = Prompt.ask("Backend", choices=list(BACKENDS), default="staging", console=console)
    if value is None:
        raise SystemExit(f"error: no backend; pass -b or set ${BACKEND_ENV}")
    if value in BACKENDS:
        return value, BACKENDS[value]
    if re.match(r"https?://", value):
        return "custom", value.rstrip("/")
    raise SystemExit(f"error: backend must be one of {', '.join(BACKENDS)} or an http(s) URL")


MENU = [
    ("s", "Status of my key", lambda s: show_status(s)),
    ("l", "List active keys", lambda s: act_list(s)),
    ("i", "Inspect a keytag (what it wrote)", lambda s: act_show(s, Prompt.ask("Keytag", console=console))),
    ("c", "Create a key", lambda s: act_create(s)),
    ("r", "Revoke key(s)", lambda s: act_revoke(s)),
    ("o", "Rotate a key", lambda s: act_rotate(s)),
    ("q", "Quit", None),
]
KEYADDER_ONLY = {"l", "i", "c", "r"}


def menu_loop(s: Session) -> None:
    while True:
        console.print()
        for k, label, _ in MENU:
            dim = s.scope != "keyadder" and k in KEYADDER_ONLY
            console.print(f"  [bold]{k}[/]  {f'[dim]{label} (keyadder only)[/]' if dim else label}")
        choice = Prompt.ask("Action", choices=[k for k, _, _ in MENU], default="s",
                            show_choices=False, console=console)
        action = next(fn for k, _, fn in MENU if k == choice)
        if action is None:
            return
        try:
            action(s)
        except ApiError as e:
            console.print(f"[red]✗[/] {e}")


def main(argv: list[str] | None = None) -> int:
    p = argparse.ArgumentParser(description="Manage PaZuFa API keys. Without a command: interactive menu.")
    p.add_argument("-b", "--backend", default=os.environ.get(BACKEND_ENV),
                   help=f"{'|'.join(BACKENDS)} or a URL (default: ${BACKEND_ENV}, else ask)")
    p.add_argument("--key-file", help=f"read the authenticating key from this file (default: ${KEY_ENV}, else ask)")
    p.add_argument("-y", "--yes", action="store_true", help="do not ask for confirmation")
    sub = p.add_subparsers(dest="command")
    sub.add_parser("status", help="show scope/expiry of your key")
    sub.add_parser("list", help="list active keytags (keyadder)")
    sp = sub.add_parser("show", help="list objects written with a keytag (keyadder)")
    sp.add_argument("keytag")
    sp = sub.add_parser("create", help="create a key (keyadder)")
    sp.add_argument("scope", choices=SCOPES)
    sp.add_argument("-e", "--expires", default="", help="90d, 12h, 2w, or ISO date/time (default: 365 days)")
    sp.add_argument("-p", "--prefix", default="", help=f"keytag prefix, ≤{KEYTAG_MAX_PREFIX_LENGTH} chars")
    sp = sub.add_parser("revoke", help="revoke keys by keytag (keyadder)")
    sp.add_argument("keytags", nargs="+")
    sub.add_parser("rotate", help="rotate your own key")
    args = p.parse_args(argv)

    name, url = resolve_backend(args.backend)
    client = Client(url, resolve_key(args.key_file))
    try:
        with console.status("Authenticating…"):
            st = client.status()
    except ApiError as e:
        console.print(f"[red]✗[/] login failed ({client.keytag}): {e}")
        return 1

    s = Session(client, name, st["scope"], args.yes, interactive=args.command is None)
    try:
        match args.command:
            case None:
                show_status(s, st)
                menu_loop(s)
            case "status":
                show_status(s, st)
            case "list":
                if s.require_keyadder("Listing keys"):
                    if sys.stdout.isatty():
                        act_list(s)
                    else:
                        print("\n".join(client.list_keytags()))
            case "show":
                act_show(s, args.keytag)
            case "create":
                act_create(s, args.scope, args.expires, args.prefix)
            case "revoke":
                act_revoke(s, args.keytags)
            case "rotate":
                act_rotate(s)
    except ApiError as e:
        console.print(f"[red]✗[/] {e}")
        return 1
    except (KeyboardInterrupt, EOFError):
        console.print("\nAborted.")
        return 130
    return 0


if __name__ == "__main__":
    try:
        sys.exit(main())
    except (KeyboardInterrupt, EOFError):
        console.print("\nAborted.")
        sys.exit(130)
