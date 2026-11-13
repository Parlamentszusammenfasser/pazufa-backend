#!/usr/bin/env bash
set -euo pipefail

IMAGE="pazufa/release-binaries:0.3.6"
ARTIFACTS=("elastic-companion" "pazufa-backend" "prefixmatch" "validate-vorgang")
PLATFORMS=("amd64" "arm64")

for PLATFORM in "${PLATFORMS[@]}"; do
    echo "Processing linux/${PLATFORM}..."

    # Options must come before the image; anything after it is the container command.
    # The image has no CMD/ENTRYPOINT, so pass a placeholder command (never executed).
    CONTAINER=$(docker create --platform "linux/${PLATFORM}" "$IMAGE" /placeholder)
    echo "Container ${CONTAINER} created on ${PLATFORM}"

    trap 'docker rm -f "$CONTAINER" >/dev/null 2>&1 || true' EXIT

    for ARTIFACT in "${ARTIFACTS[@]}"; do
        SOURCE="/${ARTIFACT}"
        TARGET="./${ARTIFACT}-linux-${PLATFORM}"

        docker cp "${CONTAINER}:${SOURCE}" "$TARGET"

        echo "  Copied ${SOURCE} -> ${TARGET}"
    done

    docker rm "$CONTAINER" >/dev/null
    trap - EXIT
done

echo "All artifacts successfully extracted."
