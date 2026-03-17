#!/usr/bin/env bash
# ──────────────────────────────────────────────────────────────────────
# APEX Terminal — Development Environment Launcher
# Starts Vite dev server and the Tauri desktop window.
# Usage:  ./scripts/dev.sh [--skip-db] [--web-only]
# ──────────────────────────────────────────────────────────────────────
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(cd "${SCRIPT_DIR}/.." && pwd)"

SKIP_DB=false
WEB_ONLY=false

while [[ $# -gt 0 ]]; do
    case "$1" in
        --skip-db)  SKIP_DB=true;  shift ;;
        --web-only) WEB_ONLY=true; shift ;;
        -h|--help)
            echo "Usage: $0 [--skip-db] [--web-only]"
            echo "  --skip-db   Skip database setup"
            echo "  --web-only  Only start Vite (no Tauri window)"
            exit 0 ;;
        *)
            echo "Unknown option: $1" >&2; exit 1 ;;
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
echo ""

# ── Cleanup trap ─────────────────────────────────────────────────────

VITE_PID=""

cleanup() {
    echo ""
    echo "Shutting down APEX dev services..."
    if [[ -n "${VITE_PID}" ]] && kill -0 "${VITE_PID}" 2>/dev/null; then
        kill "${VITE_PID}" 2>/dev/null || true
    fi
    wait 2>/dev/null || true
    echo "✓ Done."
}

trap cleanup EXIT INT TERM

# ── Start Vite dev server ─────────────────────────────────────────────

echo "Starting Vite dev server on http://localhost:3000 ..."
cd "${ROOT_DIR}/apex-ui"

if command -v pnpm &>/dev/null; then
    pnpm dev &
else
    npm run dev &
fi
VITE_PID=$!
echo "  → Vite PID: ${VITE_PID}"

# Wait until port 3000 is accepting connections (max 30s)
echo "  → Waiting for frontend to be ready..."
for i in $(seq 1 30); do
    if curl -s http://localhost:3000 >/dev/null 2>&1; then
        echo "  ✓ Frontend is ready."
        break
    fi
    sleep 1
done

# ── Launch Tauri desktop app ──────────────────────────────────────────

if [[ "${WEB_ONLY}" == false ]]; then
    echo ""
    echo "Launching APEX Terminal (Tauri)..."
    echo "  → Opening desktop window loading from http://localhost:3000"
    echo ""
    echo "═══════════════════════════════════════"
    echo "  APEX Terminal is launching!"
    echo "  Web UI  : http://localhost:3000"
    echo "  Press Ctrl+C to stop everything."
    echo "═══════════════════════════════════════"
    echo ""

    cd "${ROOT_DIR}"
    cargo run --bin apex-tauri
else
    echo ""
    echo "═══════════════════════════════════════"
    echo "  APEX web UI running (browser only)"
    echo "  Open  : http://localhost:3000"
    echo "  Press Ctrl+C to stop."
    echo "═══════════════════════════════════════"
    echo ""
    # Keep alive
    wait "${VITE_PID}"
fi
