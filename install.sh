#!/bin/sh
# QSMxT installer — downloads the latest release binary for your platform.
# Usage: curl -fsSL https://raw.githubusercontent.com/QSMxT/QSMxT/main/install.sh | sh
#
# The archive is verified against the release's SHA256SUMS before anything is
# extracted; a missing manifest, a missing entry or a mismatch aborts the install.
#
# Environment:
#   INSTALL_DIR          where to put qsmxt (default /usr/local/bin)
#   QSMXT_VERSION        install this release tag (e.g. v9.23.0) instead of the latest
#   QSMXT_DOWNLOAD_URL   base URL holding <tag>/<archive> and <tag>/SHA256SUMS
#                        (default: the GitHub release downloads; for mirrors and tests)

set -e

REPO="QSMxT/QSMxT"
INSTALL_DIR="${INSTALL_DIR:-/usr/local/bin}"
DOWNLOAD_URL="${QSMXT_DOWNLOAD_URL:-https://github.com/${REPO}/releases/download}"

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

# Pick a SHA-256 tool up front: refusing after the download is a worse experience.
if command -v sha256sum >/dev/null 2>&1; then
    sha256() { sha256sum "$1" | cut -d' ' -f1; }
elif command -v shasum >/dev/null 2>&1; then
    sha256() { shasum -a 256 "$1" | cut -d' ' -f1; }
elif command -v openssl >/dev/null 2>&1; then
    sha256() { openssl dgst -sha256 -r "$1" | cut -d' ' -f1; }
else
    echo "Error: need sha256sum, shasum or openssl to verify the download"
    exit 1
fi

# Get latest release tag
TAG="${QSMXT_VERSION:-}"
if [ -z "$TAG" ]; then
    echo "Fetching latest release..."
    CURL_OPTS="-fsSL"
    if [ -n "$GITHUB_TOKEN" ]; then
        CURL_OPTS="$CURL_OPTS -H \"Authorization: token $GITHUB_TOKEN\""
    fi
    TAG=$(eval curl $CURL_OPTS "https://api.github.com/repos/${REPO}/releases/latest" | grep '"tag_name"' | head -1 | cut -d'"' -f4)
fi

if [ -z "$TAG" ]; then
    echo "Error: could not determine latest release"
    exit 1
fi

echo "Installing qsmxt ${TAG} for ${TARGET}..."

# Download, verify, extract
ASSET="qsmxt-${TAG}-${TARGET}.tar.gz"
URL="${DOWNLOAD_URL}/${TAG}/${ASSET}"
TMPDIR=$(mktemp -d)
trap 'rm -rf "$TMPDIR"' EXIT

if ! curl -fsSL "${DOWNLOAD_URL}/${TAG}/SHA256SUMS" -o "${TMPDIR}/SHA256SUMS"; then
    echo "Error: could not download SHA256SUMS for ${TAG}; refusing to install an unverified binary"
    exit 1
fi
# Exact file-name match on the second field (`sha256sum` writes "<hex>  <file>" or "<hex> *<file>").
EXPECTED=$(awk -v f="$ASSET" '$2 == f || $2 == "*" f { print tolower($1); exit }' "${TMPDIR}/SHA256SUMS")
if [ -z "$EXPECTED" ]; then
    echo "Error: SHA256SUMS for ${TAG} has no entry for ${ASSET}; refusing to install an unverified binary"
    exit 1
fi

# Show a progress bar when attached to a terminal; stay quiet in CI/logs.
if [ -t 2 ]; then
    curl -fL --progress-bar "$URL" -o "${TMPDIR}/qsmxt.tar.gz"
else
    curl -fsSL "$URL" -o "${TMPDIR}/qsmxt.tar.gz"
fi
ACTUAL=$(sha256 "${TMPDIR}/qsmxt.tar.gz" | tr 'A-F' 'a-f')
if [ "$ACTUAL" != "$EXPECTED" ]; then
    echo "Error: checksum mismatch for ${ASSET}"
    echo "  expected (SHA256SUMS): ${EXPECTED}"
    echo "  downloaded:            ${ACTUAL}"
    echo "Nothing was installed."
    exit 1
fi
echo "Verified ${ASSET} against SHA256SUMS."
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
