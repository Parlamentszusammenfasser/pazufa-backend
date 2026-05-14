#!/usr/bin/python3
# takes the variables in variables.toml and sets all affected files to the values
# specified there. for example: Project version, API Spec version, ...
import tomllib
from pathlib import Path
import re


def replace(file: str, pattern: str, replacement: str):
    file = Path(file)
    text = file.read_text()
    new_text = re.sub(pattern, replacement, text)
    if text != new_text:
        file.write_text(new_text)
        print(f"Updated {file}")
    else:
        print(f"No changes needed in {file}")


with open("variables.toml", "rb") as f:
    file = tomllib.load(f)

version = file["version"]["version"]
trackfile = file["version"]["trackfile"]

replace(
    "docker-compose.yml",
    r"    image: pazufa/backend:\d+\.\d+\.\d+",
    f"    image: pazufa/backend:{version}",
)
replace("Dockerfile", r"LABEL version=\"\d+\.\d+\.\d+\"", f'LABEL version="{version}"')
replace(
    "Cargo.toml", 'version\\s*=\\s*"\\d+\\.\\d+\\.\\d+"\n', f'version = "{version}"\n'
)
replace(
    ".woodpecker/deploy.yml",
    r"  - &version \"\d+\.\d+\.\d+\"",
    f'  - &version "{version}"',
)


replace("Dockerfile", r"ENV TRACK_PATH=\"https://.*\"", f'ENV TRACK_PATH="{trackfile}"')
replace(
    ".woodpecker/testing.yml",
    r"      TRACK_PATH: https://.*",
    f"      TRACK_PATH: {trackfile}",
)
