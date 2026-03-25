#!/usr/bin/env bash
# Download public PPTX test fixtures listed in tests/fixtures/public_fixtures.txt
#
# Usage: ./scripts/download_fixtures.sh
#
# Downloads are placed under tests/fixtures/public/<category>/<filename>
# Already-downloaded files are skipped (delete to re-download).

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
MANIFEST="$PROJECT_DIR/tests/fixtures/public_fixtures.txt"
OUT_DIR="$PROJECT_DIR/tests/fixtures/public"

if [ ! -f "$MANIFEST" ]; then
    echo "ERROR: Manifest not found: $MANIFEST" >&2
    exit 1
fi

mkdir -p "$OUT_DIR"

total=0
downloaded=0
skipped=0
failed=0

while IFS= read -r line; do
    # Skip comments and blank lines
    [[ "$line" =~ ^[[:space:]]*# ]] && continue
    [[ -z "${line// /}" ]] && continue

    name="$(echo "$line" | awk '{print $1}')"
    url="$(echo "$line" | awk '{print $2}')"

    if [ -z "$name" ] || [ -z "$url" ]; then
        continue
    fi

    total=$((total + 1))
    dest="$OUT_DIR/$name"
    dest_dir="$(dirname "$dest")"

    if [ -f "$dest" ]; then
        skipped=$((skipped + 1))
        continue
    fi

    mkdir -p "$dest_dir"

    if curl -fsSL --retry 3 --retry-delay 2 -o "$dest" "$url" 2>/dev/null; then
        downloaded=$((downloaded + 1))
        echo "  OK  $name"
    else
        failed=$((failed + 1))
        echo "  FAIL $name ($url)"
        rm -f "$dest"  # clean up partial download
    fi
done < "$MANIFEST"

echo ""
echo "Done: $downloaded downloaded, $skipped already present, $failed failed (out of $total)"
