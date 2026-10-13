#!/bin/sh
# Usage: sha256sums.sh <dir>  — print a SHA256SUMS manifest for the release archives in <dir>.
#
# One `<sha256>  <file>` line per qsmxt-*.tar.gz / qsmxt-*.zip, sorted by name, in the
# format `sha256sum -c` reads. `qsmxt update`, install.sh and install.ps1 look up their
# archive by exact file name in this file and refuse to install on a mismatch or a
# missing entry. Fails if the directory holds no archives, so an empty manifest is never
# published.
set -eu
dir="${1:?usage: sha256sums.sh <dir>}"
cd "$dir"
found=0
for f in $(ls qsmxt-*.tar.gz qsmxt-*.zip 2>/dev/null | LC_ALL=C sort); do
    found=1
    if command -v sha256sum >/dev/null 2>&1; then
        sha256sum "$f"
    else
        shasum -a 256 "$f"
    fi
done
if [ "$found" -eq 0 ]; then
    echo "sha256sums.sh: no qsmxt-*.tar.gz or qsmxt-*.zip archives in $dir" >&2
    exit 1
fi
