#!/usr/bin/env bash
# CLI-35: measure the release artifacts and fail when one is over its budget,
# or more than 10% heavier than the previous release without a changelog note.
#
# The budgets themselves live in `crates/liyasa-cli/src/budget.rs` and reach
# this script through `liyasa budgets --json`, so there is one table rather than
# one per consumer.
#
# Usage:
#   ci/artifact-budgets.sh <artifact-dir> [previous-measurements.json]
#
# `<artifact-dir>` holds the built release artifacts. Anything the run cannot
# find is skipped and reported, because a job that silently measures nothing
# passes for the wrong reason.
set -euo pipefail

ARTIFACTS="${1:?usage: artifact-budgets.sh <artifact-dir> [previous.json]}"
PREVIOUS="${2:-}"
LIYASA="${LIYASA:-liyasa}"

measured_json="$(mktemp)"
trap 'rm -f "$measured_json"' EXIT

size_of() {
  # Compressed figures are what a reader downloads, so they are measured after
  # gzip rather than on disk.
  local path="$1" compressed="$2"
  [ -e "$path" ] || return 1
  if [ "$compressed" = "true" ]; then
    gzip -9 -c "$path" | wc -c
  else
    wc -c < "$path"
  fi
}

# Where each budget's artifact is expected to be, relative to the artifact
# directory. A row with no path here is not measured by this job.
path_for() {
  case "$1" in
    "binary (default features)") echo "liyasa" ;;
    "binary (all features)")     echo "liyasa-all-features" ;;
    "embedded web assets")       echo "web-assets.tar" ;;
    "editor wasm")               echo "editor.wasm" ;;
    "reader base javascript")    echo "reader.js" ;;
    "reader css")                echo "theme.css" ;;
    "search reader")             echo "search.js" ;;
    "docker image")              echo "image.tar" ;;
    *)                           echo "" ;;
  esac
}

previous_for() {
  [ -n "$PREVIOUS" ] && [ -f "$PREVIOUS" ] || return 1
  python3 -c "
import json,sys
rows = json.load(open('$PREVIOUS'))
print(rows.get('''$1''', ''))
" 2>/dev/null
}

failed=0
skipped=0
echo '{' > "$measured_json"
first=1

while IFS=$'\t' read -r name limit compressed requirement; do
  relative="$(path_for "$name")"
  if [ -z "$relative" ] || ! size="$(size_of "$ARTIFACTS/$relative" "$compressed")"; then
    printf '  --  %-28s not built in this run\n' "$name"
    skipped=$((skipped + 1))
    continue
  fi

  verdict="ok"
  if [ "$size" -gt "$limit" ]; then
    verdict="OVER"
    failed=1
  else
    before="$(previous_for "$name" || true)"
    if [ -n "$before" ] && [ "$before" -gt 0 ] 2>/dev/null; then
      # Integer arithmetic: grown when size * 100 > before * 110.
      if [ $((size * 100)) -gt $((before * 110)) ]; then
        verdict="GREW"
        failed=1
      fi
    fi
  fi

  printf '  %-4s %-28s %10s / %-10s  (%s)\n' "$verdict" "$name" "$size" "$limit" "$requirement"
  [ $first -eq 0 ] && echo ',' >> "$measured_json"
  first=0
  printf '  "%s": %s' "$name" "$size" >> "$measured_json"
done < <("$LIYASA" budgets --json | python3 -c "
import json,sys
for row in json.load(sys.stdin):
    print('\t'.join([row['name'], str(row['limit']), str(row['compressed']).lower(), row['requirement']]))
")

printf '\n}\n' >> "$measured_json"
cp "$measured_json" "${MEASURED_OUT:-artifact-sizes.json}"

if [ "$skipped" -gt 0 ]; then
  echo "note: $skipped artifact(s) were not built in this run"
fi
if [ "$failed" -ne 0 ]; then
  echo "artifact budgets: FAILED"
  echo "An OVER row must shrink. A GREW row needs a changelog note explaining it."
  exit 1
fi
echo "artifact budgets: green"
