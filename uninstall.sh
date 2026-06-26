#!/bin/sh
# QSMxT.rs uninstaller for Linux/macOS
# Usage: curl -fsSL https://raw.githubusercontent.com/QSMxT/QSMxT/main/uninstall.sh | sh

set -e

INSTALL_DIR="${INSTALL_DIR:-/usr/local/bin}"
BINARY="${INSTALL_DIR}/qsmxt"

if [ ! -f "$BINARY" ]; then
    echo "qsmxt not found at ${BINARY}"
    exit 0
fi

if [ -w "$INSTALL_DIR" ]; then
    rm "$BINARY"
elif command -v sudo >/dev/null 2>&1; then
    echo "Removing ${BINARY} (requires sudo)..."
    sudo rm "$BINARY"
else
    echo "Permission denied. Run manually: sudo rm ${BINARY}"
    exit 1
fi

echo "qsmxt has been removed from ${INSTALL_DIR}"

# Remove bundled dcm2niix and the ~/.qsmxt directory if present
QSMXT_DIR="${HOME}/.qsmxt"
if [ -e "${QSMXT_DIR}/bin/dcm2niix" ]; then
    rm -f "${QSMXT_DIR}/bin/dcm2niix" "${QSMXT_DIR}/bin/dcm2niix.LICENSE"
    echo "Removed bundled dcm2niix from ${QSMXT_DIR}/bin"
fi
# Remove ~/.qsmxt if it is now empty
if [ -d "$QSMXT_DIR" ]; then
    rmdir "${QSMXT_DIR}/bin" 2>/dev/null || true
    rmdir "$QSMXT_DIR" 2>/dev/null || true
fi
