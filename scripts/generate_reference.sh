#!/usr/bin/env bash
# Generate reference PNG images from PPTX fixtures using LibreOffice.
#
# Usage: ./scripts/generate_reference.sh [dpi]
#
# Requires: libreoffice (headless mode)
#
# This exports each slide of each .pptx in tests/fixtures/ as a PNG
# and places the results in tests/reference/<fixture_name>/.

set -euo pipefail

DPI="${1:-150}"
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
FIXTURES_DIR="$PROJECT_DIR/tests/fixtures"
REFERENCE_DIR="$PROJECT_DIR/tests/reference"

if ! command -v libreoffice &>/dev/null; then
    echo "ERROR: libreoffice is not installed or not in PATH" >&2
    exit 1
fi

mkdir -p "$REFERENCE_DIR"

for pptx in "$FIXTURES_DIR"/*.pptx; do
    [ -f "$pptx" ] || continue

    basename="$(basename "$pptx" .pptx)"
    out_dir="$REFERENCE_DIR/$basename"
    mkdir -p "$out_dir"

    echo "Rendering $basename at ${DPI} DPI..."

    # LibreOffice exports to the input file's directory by default.
    # We use a temp dir then move.
    tmpdir="$(mktemp -d)"

    # Copy pptx to tmpdir so libreoffice outputs there
    cp "$pptx" "$tmpdir/"

    # Export as PNG images (one per slide)
    libreoffice --headless \
        --convert-to png \
        --outdir "$tmpdir" \
        "$tmpdir/$(basename "$pptx")" \
        2>/dev/null

    # LibreOffice only produces one file for single-slide; for multi-slide
    # it may produce slide1.png, slide2.png etc. or a single file.
    # We use a Python one-liner to export individual slides at the right DPI.
    python3 -c "
import subprocess, sys, os, shutil

pptx_path = '$pptx'
out_dir = '$out_dir'
dpi = int('$DPI')

# Use LibreOffice to convert to PDF first, then use pdftoppm for per-page PNG at exact DPI
tmpdir = '$tmpdir'
pdf_path = os.path.join(tmpdir, '${basename}.pdf')

subprocess.run([
    'libreoffice', '--headless', '--convert-to', 'pdf',
    '--outdir', tmpdir, pptx_path
], check=True, capture_output=True)

if os.path.exists(pdf_path):
    # Use pdftoppm if available, otherwise fall back to ImageMagick
    try:
        subprocess.run([
            'pdftoppm', '-png', '-r', str(dpi), pdf_path,
            os.path.join(out_dir, 'slide')
        ], check=True, capture_output=True)
    except FileNotFoundError:
        # Fallback: use convert (ImageMagick)
        subprocess.run([
            'convert', '-density', str(dpi), pdf_path,
            os.path.join(out_dir, 'slide_%d.png')
        ], check=True, capture_output=True)

# Rename pdftoppm output (slide-1.png -> slide_1.png)
for f in os.listdir(out_dir):
    if f.startswith('slide-'):
        new_name = f.replace('slide-', 'slide_', 1)
        # pdftoppm pads page numbers, e.g. slide-01.png -> slide_1.png
        # Strip leading zeros from the number
        parts = new_name.split('_', 1)
        if len(parts) == 2:
            num_part = parts[1].replace('.png', '').lstrip('0') or '1'
            new_name = f'slide_{num_part}.png'
        os.rename(os.path.join(out_dir, f), os.path.join(out_dir, new_name))
"

    rm -rf "$tmpdir"
    echo "  -> $out_dir/"
done

echo "Done."
