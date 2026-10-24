#!/usr/bin/env python3
"""
Checks that the spec version in variables.toml was bumped above the
version currently published on the spec repo's main branch.

Exit codes:
    0 - TOML version is greater than the published spec version
    1 - TOML version is lower than or equal to the published spec version
    2 - error (download, parsing, invalid version)
"""
import re
import sys

import requests
import yaml

import tomllib

SPEC_URL = "https://codeberg.org/PaZuFa/parlamentszusammenfasser/raw/branch/main/openapi.yml"
TOML_FILE = "variables.toml"

SEMVER_RE = re.compile(
    r"^v?(?P<major>0|[1-9]\d*)\.(?P<minor>0|[1-9]\d*)\.(?P<patch>0|[1-9]\d*)"
    r"(?:-(?P<pre>[0-9A-Za-z.-]+))?(?:\+[0-9A-Za-z.-]+)?$"
)


def parse_semver(version: str):
    """Return a sortable key for a semantic version (build metadata is ignored)."""
    match = SEMVER_RE.match(str(version).strip())
    if not match:
        raise ValueError(f"Not a valid semantic version: {version!r}")
    core = (int(match["major"]), int(match["minor"]), int(match["patch"]))
    pre = match["pre"]
    if pre is None:
        # A release version has higher precedence than any pre-release of the same core.
        return core, 1, ()
    identifiers = []
    for part in pre.split("."):
        # Numeric identifiers sort before alphanumeric ones, as in the semver spec.
        identifiers.append((0, int(part), "") if part.isdigit() else (1, 0, part))
    return core, 0, tuple(identifiers)


def download_spec(url: str) -> dict:
    response = requests.get(url, timeout=30)
    response.raise_for_status()
    spec = yaml.safe_load(response.text)
    if not isinstance(spec, dict):
        raise ValueError("Downloaded file is not a YAML mapping")
    return spec


def read_toml_version(path: str) -> str:
    with open(path, "rb") as f:
        data = tomllib.load(f)
        return str(data["version"]["openapi-spec"])


def main() -> int:
    try:
        spec = download_spec(SPEC_URL)
        spec_version = spec["info"]["version"]
        toml_version = read_toml_version(TOML_FILE)

        spec_key = parse_semver(spec_version)
        toml_key = parse_semver(toml_version)
    except (requests.RequestException, yaml.YAMLError, KeyError, ValueError, OSError, tomllib.TOMLDecodeError) as exc:
        print(f"Error: {exc}", file=sys.stderr)
        return 2

    print(f"OpenAPI spec version: {spec_version}")
    print(f"TOML version:         {toml_version}")

    if toml_key > spec_key:
        print("This repo's version is greater than the published version.")
        return 0

    print("This repo's version is lower than or equal to the published version; bump openapi-spec in variables.toml.")
    return 1


if __name__ == "__main__":
    sys.exit(main())
