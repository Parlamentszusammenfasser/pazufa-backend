#!/usr/bin/python3
# takes the variables in variables.toml and sets all affected files to the values
# specified there. for example: Project version, API Spec version, ...
import tomllib
from pathlib import Path
import re

def replace(file: str, pattern: str, replacement: str):
    file=Path(file)
    text = file.read_text()
    new_text = re.sub(pattern, replacement, text)
    if text != new_text:
        file.write_text(new_text)
        print(f"Updated {file}")
    else:
        print(f"No changes needed in {file}")

with open("variables.toml", "rb") as f:
    file= tomllib.load(f)

version = file["version"]["version"]
trackfile = file["version"]["trackfile"]

replace("Dockerfile.deploy", r"LABEL version=\"\d+\.\d+\.\d+\"", f"LABEL version=\"{version}\"")
replace("Cargo.toml", "version\\s*=\\s*\"\\d+\\.\\d+\\.\\d+\"\n", f"version = \"{version}\"\n")
replace(".woodpecker.yml", r"  - &version \"\d+\.\d+\.\d+\"", f"  - &version \"{version}\"")


replace("Dockerfile.deploy", r"ENV TRACK_PATH=\"https://.*\"", f"ENV TRACK_PATH=\"{trackfile}\"")
replace(".woodpecker.yml", r"      TRACK_PATH: https://.*", f"      TRACK_PATH: {trackfile}")

oapid = file["openapi"]

replace("Dockerfile.deploy", r"ENV OPENAPI_GENERATOR_VERSION=\"\d+\.\d+\.\d+\"", f"ENV OPENAPI_GENERATOR_VERSION=\"{oapid["oapigen-version"]}\"")
replace("oapigen.sh", r"OPENAPI_GENERATOR_VERSION=\"\d+\.\d+\.\d+\"", f"OPENAPI_GENERATOR_VERSION=\"{oapid["oapigen-version"]}\"")
replace(".woodpecker.yml", r"      OPENAPI_GENERATOR_VERSION: \d+\.\d+\.\d+", f"      OPENAPI_GENERATOR_VERSION: {oapid["oapigen-version"]}")


replace("Dockerfile.deploy", r"ENV SPEC_PATH=\"https://.*\"", f"ENV SPEC_PATH=\"{oapid["oapi-spec"]}\"")
replace("oapigen.sh", r"SPEC_PATH=\"https://.*\"", f"SPEC_PATH=\"{oapid["oapi-spec"]}\"")
replace(".woodpecker.yml", r"      SPEC_PATH: https://.*", f"      SPEC_PATH: {oapid["oapi-spec"]}")


