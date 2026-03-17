#!/usr/bin/env bash
# ──────────────────────────────────────────────────────────────────────
# APEX Terminal — Production Build Script
# Builds the frontend and packages the desktop app.
#
# Usage:  ./scripts/build.sh [options]
#
#   --bundle-only   Skip cargo checks, just package (assumes prior build)
#   --no-bundle     Build release binary only — no DMG/installer
#   --target <T>    Cross-compile for target triple (e.g. aarch64-apple-darwin)
#   --skip-db       Skip database setup
#   -h, --help      Show this message
# ──────────────────────────────────────────────────────────────────────
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(cd "${SCRIPT_DIR}/.." && pwd)"

NO_BUNDLE=false
BUNDLE_ONLY=false
TARGET_FLAG=""
SKIP_DB=false

while [[ $# -gt 0 ]]; do
    case "$1" in
        --no-bundle)     NO_BUNDLE=true;      shift ;;
        --bundle-only)   BUNDLE_ONLY=true;    shift ;;
        --target)        TARGET_FLAG="$2";    shift 2 ;;
        --skip-db)       SKIP_DB=true;        shift ;;
        -h|--help)
            sed -n '3,15p' "$0"   # Print the usage block above
            exit 0 ;;
        *)
            echo "Unknown option: $1" >&2; exit 1 ;;
    esac
done

# ── Colour helpers ────────────────────────────────────────────────────
RED='\033[0;31m'; GREEN='\033[0;32m'; YELLOW='\033[1;33m'
BOLD='\033[1m';   RESET='\033[0m'

log()  { echo -e "${BOLD}[APEX]${RESET} $*"; }
ok()   { echo -e "${GREEN}  ✓${RESET} $*"; }
warn() { echo -e "${YELLOW}  ⚠${RESET} $*"; }
die()  { echo -e "${RED}  ✗ ERROR:${RESET} $*" >&2; exit 1; }

# ── Banner ────────────────────────────────────────────────────────────
echo ""
echo -e "${BOLD}╔══════════════════════════════════════════╗${RESET}"
echo -e "${BOLD}║   APEX Terminal — Production Build       ║${RESET}"
echo -e "${BOLD}╚══════════════════════════════════════════╝${RESET}"
echo ""

# ── Pre-flight checks ─────────────────────────────────────────────────
log "Checking build prerequisites..."

command -v cargo  &>/dev/null || die "cargo not found. Install Rust: https://rustup.rs"
command -v node   &>/dev/null || die "node not found. Install Node.js: https://nodejs.org"
command -v pnpm   &>/dev/null || die "pnpm not found. Install: npm i -g pnpm"

CARGO_VER=$(cargo --version)
NODE_VER=$(node --version)
PNPM_VER=$(pnpm --version)

ok "cargo  : ${CARGO_VER}"
ok "node   : ${NODE_VER}"
ok "pnpm   : v${PNPM_VER}"
echo ""

# ── Config ────────────────────────────────────────────────────────────
CONFIG_FILE="${ROOT_DIR}/config/apex.toml"
if [[ ! -f "${CONFIG_FILE}" ]]; then
    log "Creating default config..."
    cp "${ROOT_DIR}/config/apex.example.toml" "${CONFIG_FILE}"
    ok "Config written to ${CONFIG_FILE}"
fi

# ── Database schema ───────────────────────────────────────────────────
if [[ "${SKIP_DB}" == false ]]; then
    log "Setting up database schema..."
    bash "${SCRIPT_DIR}/setup_db.sh"
    ok "Database ready."
    echo ""
fi

# ── Icon generation ───────────────────────────────────────────────────
ICONS_DIR="${ROOT_DIR}/apex-tauri/icons"
SOURCE_ICON="${ICONS_DIR}/app-icon.png"

if [[ -f "${SOURCE_ICON}" ]]; then
    log "Generating app icons from ${SOURCE_ICON}..."
    cd "${ROOT_DIR}/apex-ui"
    pnpm exec tauri icon "${SOURCE_ICON}" --config ../apex-tauri/tauri.conf.json 2>/dev/null && \
        ok "Icons generated." || \
        warn "Icon generation skipped (using existing placeholders)."
    echo ""
fi

# ── Build frontend ────────────────────────────────────────────────────
log "Building frontend (TypeScript + Vite)..."
cd "${ROOT_DIR}/apex-ui"
pnpm install --frozen-lockfile 2>/dev/null || pnpm install
pnpm run build
ok "Frontend built → apex-ui/dist/"
echo ""

# ── Run Rust tests before packaging ──────────────────────────────────
if [[ "${BUNDLE_ONLY}" == false ]]; then
    log "Running Rust test suite..."
    cd "${ROOT_DIR}"
    cargo test --workspace 2>&1 | tail -8
    ok "All tests passed."
    echo ""
fi

# ── Build Tauri desktop app ───────────────────────────────────────────
cd "${ROOT_DIR}/apex-ui"

if [[ "${NO_BUNDLE}" == true ]]; then
    # ── Release binary only (fast, no installer) ──────────────────────
    log "Building release binary (--no-bundle)..."
    cd "${ROOT_DIR}"
    CARGO_ARGS="--release --bin apex-tauri"
    [[ -n "${TARGET_FLAG}" ]] && CARGO_ARGS="${CARGO_ARGS} --target ${TARGET_FLAG}"
    # shellcheck disable=SC2086
    cargo build ${CARGO_ARGS}

    BIN_PATH="${ROOT_DIR}/target/release/apex-tauri"
    [[ -n "${TARGET_FLAG}" ]] && \
        BIN_PATH="${ROOT_DIR}/target/${TARGET_FLAG}/release/apex-tauri"

    ok "Binary built: ${BIN_PATH}"
else
    # ── Full packaged app (DMG / deb / NSIS) ─────────────────────────
    log "Building Tauri app bundle (this will take a few minutes)..."
    cd "${ROOT_DIR}/apex-ui"

    TAURI_ARGS="--config ../apex-tauri/tauri.conf.json"
    [[ -n "${TARGET_FLAG}" ]] && TAURI_ARGS="${TAURI_ARGS} --target ${TARGET_FLAG}"

    # shellcheck disable=SC2086
    pnpm exec tauri build ${TAURI_ARGS}

    ok "App bundle built."
fi

# ── Report outputs ────────────────────────────────────────────────────
echo ""
echo -e "${BOLD}╔══════════════════════════════════════════╗${RESET}"
echo -e "${BOLD}║         Build Complete — Outputs         ║${RESET}"
echo -e "${BOLD}╚══════════════════════════════════════════╝${RESET}"
echo ""

BUNDLE_DIR="${ROOT_DIR}/target/release/bundle"

if [[ "${NO_BUNDLE}" == true ]]; then
    echo -e "  Binary  : ${GREEN}${ROOT_DIR}/target/release/apex-tauri${RESET}"
else
    echo -e "  Bundles : ${GREEN}${BUNDLE_DIR}/${RESET}"
    echo ""

    # macOS
    if [[ -d "${BUNDLE_DIR}/macos" ]]; then
        find "${BUNDLE_DIR}/macos" -name "*.app" | while read -r f; do
            echo -e "  macOS .app : ${GREEN}${f}${RESET}"
        done
    fi
    if [[ -d "${BUNDLE_DIR}/dmg" ]]; then
        find "${BUNDLE_DIR}/dmg" -name "*.dmg" | while read -r f; do
            echo -e "  macOS .dmg : ${GREEN}${f}${RESET}"
        done
    fi

    # Linux
    if [[ -d "${BUNDLE_DIR}/deb" ]]; then
        find "${BUNDLE_DIR}/deb" -name "*.deb" | while read -r f; do
            echo -e "  Linux .deb : ${GREEN}${f}${RESET}"
        done
    fi
    if [[ -d "${BUNDLE_DIR}/appimage" ]]; then
        find "${BUNDLE_DIR}/appimage" -name "*.AppImage" | while read -r f; do
            echo -e "  Linux .AppImage : ${GREEN}${f}${RESET}"
        done
    fi

    # Windows
    if [[ -d "${BUNDLE_DIR}/msi" ]]; then
        find "${BUNDLE_DIR}/msi" -name "*.msi" | while read -r f; do
            echo -e "  Windows .msi : ${GREEN}${f}${RESET}"
        done
    fi
    if [[ -d "${BUNDLE_DIR}/nsis" ]]; then
        find "${BUNDLE_DIR}/nsis" -name "*.exe" | while read -r f; do
            echo -e "  Windows .exe : ${GREEN}${f}${RESET}"
        done
    fi
fi

echo ""
log "Done. Run the app:"
if [[ "${NO_BUNDLE}" == true ]]; then
    echo ""
    echo "    ${ROOT_DIR}/target/release/apex-tauri"
else
    if [[ "$(uname)" == "Darwin" ]]; then
        APP=$(find "${BUNDLE_DIR}/macos" -name "*.app" 2>/dev/null | head -1)
        [[ -n "${APP}" ]] && echo "    open \"${APP}\"" || true
    fi
fi
echo ""
