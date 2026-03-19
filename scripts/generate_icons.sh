#!/usr/bin/env bash
# ──────────────────────────────────────────────────────────────────────
# APEX Terminal — Icon Generation Script
# Generates all required icon sizes from the source SVG for Tauri bundling.
#
# Requirements: One of:
#   - `rsvg-convert` (librsvg, best quality)
#   - `inkscape` (Inkscape CLI)
#   - `convert` (ImageMagick)
#
# Usage:  ./scripts/generate_icons.sh
# ──────────────────────────────────────────────────────────────────────
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(dirname "$SCRIPT_DIR")"
ICON_DIR="${REPO_ROOT}/apex-tauri/icons"
SOURCE_SVG="${ICON_DIR}/app-icon.svg"

echo "╔══════════════════════════════════════╗"
echo "║    APEX — Icon Generation            ║"
echo "╚══════════════════════════════════════╝"
echo ""

if [[ ! -f "$SOURCE_SVG" ]]; then
    echo "Error: Source SVG not found at ${SOURCE_SVG}"
    exit 1
fi

# Find a suitable SVG → PNG converter
CONVERTER=""
if command -v rsvg-convert &>/dev/null; then
    CONVERTER="rsvg"
    echo "Using rsvg-convert for SVG → PNG conversion"
elif command -v inkscape &>/dev/null; then
    CONVERTER="inkscape"
    echo "Using Inkscape for SVG → PNG conversion"
elif command -v convert &>/dev/null; then
    CONVERTER="imagemagick"
    echo "Using ImageMagick for SVG → PNG conversion"
else
    echo "Error: No SVG converter found."
    echo "Install one of: librsvg2-bin, inkscape, or imagemagick"
    echo "  apt: sudo apt install librsvg2-bin"
    echo "  brew: brew install librsvg"
    exit 1
fi

# Generate PNG at a specific size
generate_png() {
    local size=$1
    local output=$2
    
    case "$CONVERTER" in
        rsvg)
            rsvg-convert -w "$size" -h "$size" "$SOURCE_SVG" -o "$output"
            ;;
        inkscape)
            inkscape -w "$size" -h "$size" "$SOURCE_SVG" -o "$output" 2>/dev/null
            ;;
        imagemagick)
            convert -background none -resize "${size}x${size}" "$SOURCE_SVG" "$output"
            ;;
    esac
    
    echo "  ✓ Generated $(basename "$output") (${size}×${size})"
}

echo ""
echo "Generating PNG icons..."
generate_png 32  "${ICON_DIR}/32x32.png"
generate_png 128 "${ICON_DIR}/128x128.png"
generate_png 256 "${ICON_DIR}/128x128@2x.png"

# Generate ICO (Windows) — requires ImageMagick `convert` or `icotool`
if command -v convert &>/dev/null; then
    echo ""
    echo "Generating Windows .ico..."
    # Create multiple sizes for ICO
    for S in 16 32 48 256; do
        generate_png "$S" "/tmp/apex_icon_${S}.png"
    done
    convert /tmp/apex_icon_16.png /tmp/apex_icon_32.png /tmp/apex_icon_48.png /tmp/apex_icon_256.png "${ICON_DIR}/icon.ico"
    rm -f /tmp/apex_icon_*.png
    echo "  ✓ Generated icon.ico"
elif command -v icotool &>/dev/null; then
    echo ""
    echo "Generating Windows .ico with icotool..."
    for S in 16 32 48 256; do
        generate_png "$S" "/tmp/apex_icon_${S}.png"
    done
    icotool -c /tmp/apex_icon_16.png /tmp/apex_icon_32.png /tmp/apex_icon_48.png /tmp/apex_icon_256.png -o "${ICON_DIR}/icon.ico"
    rm -f /tmp/apex_icon_*.png
    echo "  ✓ Generated icon.ico"
else
    echo "  ⚠ Skipping .ico generation (need ImageMagick or icoutils)"
fi

# Generate ICNS (macOS) — requires `iconutil` (macOS) or `png2icns`
if command -v iconutil &>/dev/null; then
    echo ""
    echo "Generating macOS .icns..."
    ICONSET="/tmp/apex.iconset"
    mkdir -p "$ICONSET"
    for S in 16 32 64 128 256 512; do
        generate_png "$S" "${ICONSET}/icon_${S}x${S}.png"
    done
    for S in 16 32 64 128 256; do
        local S2=$((S * 2))
        generate_png "$S2" "${ICONSET}/icon_${S}x${S}@2x.png"
    done
    iconutil -c icns -o "${ICON_DIR}/icon.icns" "$ICONSET"
    rm -rf "$ICONSET"
    echo "  ✓ Generated icon.icns"
elif command -v png2icns &>/dev/null; then
    echo ""
    echo "Generating macOS .icns with png2icns..."
    generate_png 512 "/tmp/apex_icon_512.png"
    generate_png 256 "/tmp/apex_icon_256.png"
    generate_png 128 "/tmp/apex_icon_128.png"
    generate_png 32  "/tmp/apex_icon_32.png"
    generate_png 16  "/tmp/apex_icon_16.png"
    png2icns "${ICON_DIR}/icon.icns" /tmp/apex_icon_512.png /tmp/apex_icon_256.png /tmp/apex_icon_128.png /tmp/apex_icon_32.png /tmp/apex_icon_16.png
    rm -f /tmp/apex_icon_*.png
    echo "  ✓ Generated icon.icns"
else
    echo "  ⚠ Skipping .icns generation (need iconutil on macOS or png2icns)"
fi

echo ""
echo "Done! Icons generated in: ${ICON_DIR}"
ls -la "${ICON_DIR}/"
