#!/usr/bin/env bash
# CLI-32: the release artifact runs on the platform it was built for, with no
# shared library it cannot count on.
#
# Usage: ci/release-smoke.sh <path-to-liyasa>
set -euo pipefail

BINARY="${1:?usage: release-smoke.sh <path-to-liyasa>}"

echo "--- it runs"
"$BINARY" --version
"$BINARY" version --json > /dev/null

echo "--- it scaffolds, builds, and validates"
work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT
(
  cd "$work"
  "$BINARY" new docs --yes
  cd docs
  "$BINARY" build
  "$BINARY" validate
  test -f dist/index.html
)

echo "--- it completes for every shell"
for shell in bash zsh fish powershell elvish; do
  "$BINARY" completions "$shell" > /dev/null
done

echo "--- it is statically linked"
case "$(uname -s)" in
  Linux)
    # `ldd` on a static binary says so rather than listing anything.
    if ldd "$BINARY" 2>&1 | grep -qiE 'not a dynamic executable|statically linked'; then
      echo "static"
    else
      echo "release-smoke: the binary is dynamically linked:"
      ldd "$BINARY"
      exit 1
    fi
    ;;
  Darwin)
    # macOS always links libSystem; anything else is a dependency the release
    # cannot assume.
    if otool -L "$BINARY" | tail -n +2 | grep -vqE 'libSystem|libc\+\+|libresolv'; then
      echo "release-smoke: unexpected dynamic dependency:"
      otool -L "$BINARY"
      exit 1
    fi
    echo "static apart from the system libraries"
    ;;
  *)
    echo "no linkage check for $(uname -s)"
    ;;
esac

echo "release-smoke: green"
