#!/usr/bin/env bash
# ──────────────────────────────────────────────────────────────────────
# APEX Terminal — Development Environment Launcher
# Starts all services needed for local development.
# Usage:  ./scripts/dev.sh [--skip-db] [--release]
# ──────────────────────────────────────────────────────────────────────
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(cd "${SCRIPT_DIR}/.." && pwd)"

SKIP_DB=false
CARGO_PROFILE="dev"

while [[ $# -gt 0 ]]; do
    case "$1" in
        --skip-db)
            SKIP_DB=true
            shift
            ;;
        --release)
            CARGO_PROFILE="release"
            shift
            ;;
        -h|--help)
            echo "Usage: $0 [--skip-db] [--release]"
            echo "  --skip-db   Skip database setup"
            echo "  --release   Build Rust backend in release mode"
            exit 0
            ;;
        *)
            echo "Unknown option: $1" >&2
            exit 1
            ;;
    esac
done

echo "╔══════════════════════════════════════╗"
echo "║    APEX — Development Environment    ║"
echo "╚══════════════════════════════════════╝"
echo ""

# ── Pre-flight checks ────────────────────────────────────────────────

check_cmd() {
    if ! command -v "$1" &>/dev/null; then
        echo "Error: $1 is required but not installed." >&2
        return 1
    fi
}

echo "Checking prerequisites..."
check_cmd cargo
check_cmd node
check_cmd pnpm || check_cmd npm
echo "✓ All prerequisites found."
echo ""

# ── Config ───────────────────────────────────────────────────────────

CONFIG_FILE="${ROOT_DIR}/config/apex.toml"
if [[ ! -f "${CONFIG_FILE}" ]]; then
    echo "Config not found. Copying example config..."
    cp "${ROOT_DIR}/config/apex.example.toml" "${CONFIG_FILE}"
    echo "✓ Created ${CONFIG_FILE}"
fi

# ── Database ─────────────────────────────────────────────────────────

if [[ "${SKIP_DB}" == false ]]; then
    echo ""
    echo "Setting up database..."
    bash "${SCRIPT_DIR}/setup_db.sh"
fi

# ── Install frontend dependencies ────────────────────────────────────

echo ""
echo "Installing frontend dependencies..."
cd "${ROOT_DIR}/apex-ui"

if command -v pnpm &>/dev/null; then
    pnpm install --frozen-lockfile 2>/dev/null || pnpm install
else
    npm install
fi
echo "✓ Frontend dependencies installed."

# ── Trap for cleanup ─────────────────────────────────────────────────

PIDS=()

cleanup() {
    echo ""
    echo "Shutting down APEX dev services..."
    for pid in "${PIDS[@]}"; do
        if kill -0 "$pid" 2>/dev/null; then
            kill "$pid" 2>/dev/null || true
        fi
    done
    wait 2>/dev/null || true
    echo "✓ All services stopped."
}

trap cleanup EXIT INT TERM

# ── Start Vite dev server ────────────────────────────────────────────

echo ""
echo "Starting Vite dev server (apex-ui)..."
cd "${ROOT_DIR}/apex-ui"

if command -v pnpm &>/dev/null; then
    pnpm dev &
else
    npm run dev &
fi
PIDS+=($!)
echo "  → Vite PID: ${PIDS[-1]}"

# ── Build & run Rust backend ─────────────────────────────────────────

echo ""
echo "Building Rust backend (${CARGO_PROFILE} mode)..."
cd "${ROOT_DIR}"

if [[ "${CARGO_PROFILE}" == "release" ]]; then
    cargo build --release 2>&1 | tail -5 &
else
    cargo build 2>&1 | tail -5 &
fi
PIDS+=($!)
echo "  → Cargo PID: ${PIDS[-1]}"

# ── Wait ─────────────────────────────────────────────────────────────

echo ""
echo "═══════════════════════════════════════"
echo "  APEX dev environment is starting..."
echo "  Frontend : http://localhost:3000"
echo "  Press Ctrl+C to stop all services."
echo "═══════════════════════════════════════"
echo ""

wait
