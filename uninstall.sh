#!/bin/sh
# QSMxT uninstaller for Linux/macOS
# Usage: curl -fsSL https://raw.githubusercontent.com/QSMxT/QSMxT/main/uninstall.sh | sh

set -e

# Without INSTALL_DIR, remove the qsmxt on PATH, else look where install.sh puts it.
if [ -z "${INSTALL_DIR:-}" ]; then
    FOUND=$(command -v qsmxt 2>/dev/null || true)
    case "$FOUND" in
        /*) INSTALL_DIR=$(dirname "$FOUND") ;;
        *)  if [ -f "${HOME}/.local/bin/qsmxt" ]; then INSTALL_DIR="${HOME}/.local/bin"; else INSTALL_DIR=/usr/local/bin; fi ;;
    esac
fi
BINARY="${INSTALL_DIR}/qsmxt"

PATH_MARKER="# added by the QSMxT installer"

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

# Remove the PATH line install.sh added to shell startup files
for RC in "${ZDOTDIR:-$HOME}/.zshrc" "$HOME/.bashrc" "$HOME/.bash_profile" "$HOME/.profile" \
          "${XDG_CONFIG_HOME:-$HOME/.config}/fish/config.fish"; do
    if [ -f "$RC" ] && grep -qF "$PATH_MARKER" "$RC"; then
        TMP=$(mktemp)
        grep -vF "$PATH_MARKER" "$RC" > "$TMP" || true
        cat "$TMP" > "$RC"   # rewrite in place: keeps the file's permissions and any symlink
        rm -f "$TMP"
        echo "Removed the QSMxT PATH entry from ${RC}"
    fi
done
