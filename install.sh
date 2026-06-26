#!/bin/sh
# QSMxT.rs installer — downloads the latest release binary for your platform.
# Usage: curl -fsSL https://raw.githubusercontent.com/QSMxT/QSMxT/main/install.sh | sh

set -e

REPO="QSMxT/QSMxT"
INSTALL_DIR="${INSTALL_DIR:-/usr/local/bin}"

# Detect OS and architecture
OS="$(uname -s)"
ARCH="$(uname -m)"

case "$OS" in
    Linux)
        case "$ARCH" in
            x86_64)  TARGET="x86_64-unknown-linux-musl" ;;
            aarch64) TARGET="aarch64-unknown-linux-gnu" ;;
            *) echo "Unsupported architecture: $ARCH"; exit 1 ;;
        esac
        ;;
    Darwin)
        case "$ARCH" in
            x86_64)  TARGET="x86_64-apple-darwin" ;;
            arm64)   TARGET="aarch64-apple-darwin" ;;
            *) echo "Unsupported architecture: $ARCH"; exit 1 ;;
        esac
        ;;
    *)
        echo "Unsupported OS: $OS (use install.ps1 for Windows)"
        exit 1
        ;;
esac

# Get latest release tag
echo "Fetching latest release..."
CURL_OPTS="-fsSL"
if [ -n "$GITHUB_TOKEN" ]; then
    CURL_OPTS="$CURL_OPTS -H \"Authorization: token $GITHUB_TOKEN\""
fi
TAG=$(eval curl $CURL_OPTS "https://api.github.com/repos/${REPO}/releases/latest" | grep '"tag_name"' | head -1 | cut -d'"' -f4)

if [ -z "$TAG" ]; then
    echo "Error: could not determine latest release"
    exit 1
fi

echo "Installing qsmxt ${TAG} for ${TARGET}..."

# Download and extract
URL="https://github.com/${REPO}/releases/download/${TAG}/qsmxt-${TAG}-${TARGET}.tar.gz"
TMPDIR=$(mktemp -d)
trap 'rm -rf "$TMPDIR"' EXIT

curl -fsSL "$URL" -o "${TMPDIR}/qsmxt.tar.gz"
tar xzf "${TMPDIR}/qsmxt.tar.gz" -C "$TMPDIR"

# Install
mkdir -p "$INSTALL_DIR"
if [ -w "$INSTALL_DIR" ]; then
    mv "${TMPDIR}/qsmxt" "${INSTALL_DIR}/qsmxt"
else
    echo "Installing to ${INSTALL_DIR} (requires sudo)..."
    sudo mv "${TMPDIR}/qsmxt" "${INSTALL_DIR}/qsmxt"
fi

chmod +x "${INSTALL_DIR}/qsmxt"
echo "Installed qsmxt ${TAG} to ${INSTALL_DIR}/qsmxt"

# Install bundled dcm2niix (present for x86_64-linux/macOS; ARM builds rely on PATH)
if [ -f "${TMPDIR}/dcm2niix" ]; then
    QSMXT_BIN="${HOME}/.qsmxt/bin"
    mkdir -p "$QSMXT_BIN"
    mv "${TMPDIR}/dcm2niix" "${QSMXT_BIN}/dcm2niix"
    chmod +x "${QSMXT_BIN}/dcm2niix"
    [ -f "${TMPDIR}/dcm2niix.LICENSE" ] && mv "${TMPDIR}/dcm2niix.LICENSE" "${QSMXT_BIN}/dcm2niix.LICENSE"
    echo "Installed bundled dcm2niix to ${QSMXT_BIN}/dcm2niix"
fi

echo ""
echo "Run 'qsmxt --version' to verify, or 'qsmxt tui' to get started."
