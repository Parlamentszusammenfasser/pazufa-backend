#!/usr/bin/python3

from pathlib import Path

import re
import sys
import tomllib

# takes the variables in variables.toml and sets all affected files to the values
# specified there. for example: Project version, API Spec version, ...
STATE_IS_DIRTY = False


def replace(file: str, pattern: str, replacement: str):
    global STATE_IS_DIRTY

    fpath = Path(file)
    text = fpath.read_text()
    if not re.search(pattern, text):
        STATE_IS_DIRTY = True
        print(f"Error: Pattern /{pattern}/ for file `{file}` did not match anything")
        sys.exit(2)
    new_text = re.sub(pattern, replacement, text)
    if text != new_text:
        fpath.write_text(new_text)
        print(f"Updated {file}")
        STATE_IS_DIRTY = True
    else:
        print(f"No changes needed in {file}")


with open("variables.toml", "rb") as f:
    file = tomllib.load(f)

version = file["version"]["backend"]
ec_version = file["version"]["elastic-companion"]
spec_version = file["version"]["openapi-spec"]
wp_dockerfile_version = file["version"]["test-dockerfile"]

rust_version = file["dependency-version"]["rustc"]

trackfile = file["resources"]["trackfile"]
swagger_ui_download_url = file["resources"]["cached-swagger-ui"]
database_state_url = file["resources"]["testing-database-state"]

rel_artifacts = " ".join(sorted(file["artifacts"]["bin-deploy"]))

# spec version
replace(
    "pazufa-backend-lib/src/utils/status.rs",
    r'pub const SPEC_VERSION: &str = "\d+\.\d+\.\d+";',
    f'pub const SPEC_VERSION: &str = "{spec_version}";',
)

# backend version
replace(
    "docker-compose.yml",
    r"    image: pazufa/backend:\d+\.\d+\.\d+",
    f"    image: pazufa/backend:{version}",
)
replace(
    "docker/Dockerfile",
    '# backend-label[\n\r \v\t\f]+LABEL version="\\d+\\.\\d+\\.\\d+"',
    f'# backend-label\nLABEL version="{version}"',
)
replace(
    "pazufa-backend/Cargo.toml",
    'version\\s*=\\s*"\\d+\\.\\d+\\.\\d+"\n',
    f'version = "{version}"\n',
)
replace(
    "pazufa-backend-lib/Cargo.toml",
    'version\\s*=\\s*"\\d+\\.\\d+\\.\\d+"\n',
    f'version = "{version}"\n',
)
replace(
    ".woodpecker/deploy.yml",
    r"  - &version \"\d+\.\d+\.\d+\"",
    f'  - &version "{version}"',
)

# ec version
replace(
    "elastic-companion/Cargo.toml",
    'version\\s*=\\s*"\\d+\\.\\d+\\.\\d+"\n',
    f'version = "{ec_version}"\n',
)
replace(
    "docker-compose.yml",
    r"    image: pazufa/elastic-companion:\d+\.\d+\.\d+",
    f"    image: pazufa/elastic-companion:{ec_version}",
)
replace(
    "docker/Dockerfile",
    '# ec-label[\n\r \v\t\f]+LABEL version="\\d+\\.\\d+\\.\\d+"',
    f'# ec-label\nLABEL version="{ec_version}"',
)
replace(
    ".woodpecker/deploy.yml",
    r"  - &companion_version \"\d+\.\d+\.\d+\"",
    f'  - &companion_version "{ec_version}"',
)

# track file
replace(
    "docker/Dockerfile",
    r"ENV TRACK_PATH=\"https://.*\"",
    f'ENV TRACK_PATH="{trackfile}"',
)
replace(
    ".woodpecker/testing.yml",
    r"  - &trackpath \"https://.*",
    f'  - &trackpath "{trackfile}"',
)


# swagger_ui download url
replace(
    ".woodpecker/testing.yml",
    r"  - &swagger_ui_download_url \"https://.*",
    f'  - &swagger_ui_download_url "{swagger_ui_download_url}"',
)
replace(
    ".woodpecker/staging.yml",
    r"  - &swagger_ui_download_url \"https://.*",
    f'  - &swagger_ui_download_url "{swagger_ui_download_url}"',
)
replace(
    ".woodpecker/deploy.yml",
    r"  - &swagger_ui_download_url \"https://.*",
    f'  - &swagger_ui_download_url "{swagger_ui_download_url}"',
)

# legacy db state
replace(
    "docker/woodpecker.Dockerfile",
    r"ARG DATABASE_STATE_URL=\"https://.*\"",
    f'ARG DATABASE_STATE_URL="{database_state_url}"',
)


# woodpecker rust image version
replace(
    "docker/woodpecker.Dockerfile",
    r"LABEL version=\".*\"",
    f'LABEL version="{wp_dockerfile_version}"',
)
replace(
    ".woodpecker/testing.yml",
    r"  - &rust_image pazufa/rust-woodpecker:.*",
    f"  - &rust_image pazufa/rust-woodpecker:{wp_dockerfile_version}",
)
replace(
    ".woodpecker/deploy-rust-woodpecker.yml",
    r"  - &version \".*\"",
    f'  - &version "{wp_dockerfile_version}"',
)
replace(
    ".woodpecker/speccheck.yml",
    r"  - &rust_image pazufa/rust-woodpecker.*",
    f"  - &rust_image pazufa/rust-woodpecker:{wp_dockerfile_version}",
)
replace(
    ".woodpecker/audit.yml",
    r"  - &rust_image pazufa/rust-woodpecker.*",
    f"  - &rust_image pazufa/rust-woodpecker:{wp_dockerfile_version}",
)

# rust toolchain version used in docker builds
replace(
    "docker/woodpecker.Dockerfile",
    r"FROM rust:.*-bookworm AS base",
    f"FROM rust:{rust_version}-bookworm AS base",
)

replace(
    "docker/Dockerfile",
    r"FROM rust:.*-slim-bookworm AS builder",
    f"FROM rust:{rust_version}-slim-bookworm AS builder",
)

# deployment artifacts

replace(
    "docker/Dockerfile",
    r'ARG RELEASE_ARTIFACTS=".*"',
    f'ARG RELEASE_ARTIFACTS="{rel_artifacts}"',
)

if STATE_IS_DIRTY:
    sys.exit(1)
