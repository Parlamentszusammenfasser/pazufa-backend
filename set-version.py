#!/usr/bin/python3

# takes the variables in variables.toml and sets all affected files to the values
# specified there. for example: Project version, API Spec version, ...


import re
import tomllib

from pathlib import Path


def replace(file: str, pattern: str, replacement: str):
    fpath = Path(file)
    text = fpath.read_text()
    new_text = re.sub(pattern, replacement, text)
    if text != new_text:
        fpath.write_text(new_text)
        print(f"Updated {file}")
    else:
        print(f"No changes needed in {file}")


with open("variables.toml", "rb") as f:
    file = tomllib.load(f)

version = file["version"]["backend_version"]
ec_version = file["version"]["ec_version"]
trackfile = file["version"]["trackfile"]
spec_version = file["version"]["spec_version"]

swagger_ui_download_url = file["external"]["swagger_ui_download_url"]


# spec version
replace(
    "pazufa-backend/src/main.rs",
    '\n\\s+version\\s*=\\s*"\\d+\\.\\d+\\.\\d+.*",',
    f'\n        version = "{spec_version}",',
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
    r"  - &ec_version \"\d+\.\d+\.\d+\"",
    f'  - &ec_version "{ec_version}"',
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
    f'  - &swagger_ui_download_url "{swagger_ui_download_url}"'
)
replace(
    ".woodpecker/staging.yml",
    r"  - &swagger_ui_download_url \"https://.*",
    f'  - &swagger_ui_download_url "{swagger_ui_download_url}"'
)
replace(
    ".woodpecker/deploy.yml",
    r"  - &swagger_ui_download_url \"https://.*",
    f'  - &swagger_ui_download_url "{swagger_ui_download_url}"'
)
