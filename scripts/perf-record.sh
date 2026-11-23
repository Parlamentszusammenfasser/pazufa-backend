#!/bin/bash
# Attach perf to a running process, record until Ctrl+C, then render a flamegraph.
set -euo pipefail

USAGE="Usage: $0 <process-name> <name-of-this-record>"

if [ $# -ne 2 ]; then
    echo "Wrong number of arguments. $USAGE" >&2
    exit 1
fi

RECORD_PROGRAM=$1
RECORD_NAME=$2

# Match the exact process name (not the full cmdline, which would also match this script).
mapfile -t PIDS < <(pgrep -x "$RECORD_PROGRAM" || true)

if [ ${#PIDS[@]} -eq 0 ]; then
    echo "No running process named '$RECORD_PROGRAM' found." >&2
    exit 1
elif [ ${#PIDS[@]} -gt 1 ]; then
    echo "Multiple processes named '$RECORD_PROGRAM' found:" >&2
    ps -o pid=,args= -p "$(IFS=,; echo "${PIDS[*]}")" >&2
    exit 1
fi

PID=${PIDS[0]}
echo "Attaching to '$RECORD_PROGRAM' (PID $PID). Press Ctrl+C to stop recording."

# Ctrl+C should stop perf, not this script, so the flamegraph still gets rendered.
trap ':' INT

# DWARF unwinding: the binary is not built with frame pointers, so --call-graph fp
# would produce broken stacks.
perf record -F 99 --call-graph dwarf -p "$PID" -o "$RECORD_NAME.data" || true

trap - INT

perf script -i "$RECORD_NAME.data" \
    | inferno-collapse-perf \
    | inferno-flamegraph > "$RECORD_NAME.svg"

echo "Wrote $RECORD_NAME.data, $RECORD_NAME.folded and $RECORD_NAME.svg"
