#!/usr/bin/env bash
# Build the legacy QSMxT 8.x (Python) docs site into a subfolder of the current
# site, so it stays reachable at https://qsmxt.github.io/QSMxT/v8/.
#
# The 8.x docs are a Jekyll (just-the-docs) site living in docs/ on the
# `python-legacy` branch. GitHub Pages used to build them straight from that
# folder; since v9 the same URL is served by the Astro site in this directory,
# so the docs workflow builds the old site here and copies it in under v8/.
#
# Usage: docs/legacy/build.sh <python-legacy checkout> <output dir>
#
# Needs Ruby + Bundler with the gems from ./Gemfile installed
# (`bundle install` in this directory).
set -euo pipefail

HERE=$(cd "$(dirname "$0")" && pwd)
SRC=${1:?path to a checkout of the python-legacy branch}
OUT=${2:?output directory (e.g. docs/dist/v8)}
OUT=$(mkdir -p "$OUT" && cd "$OUT" && pwd)

WORK=$(mktemp -d)
trap 'rm -rf "$WORK"' EXIT
cp -r "$SRC/docs/." "$WORK/"

# A few 8.x pages hard-code site-root links assuming the site lives at /QSMxT/.
grep -rl '](/QSMxT/' "$WORK" --include='*.md' | xargs -r sed -i 's#](/QSMxT/#](/QSMxT/v8/#g'

# Banner pointing readers at the current docs (just-the-docs includes this hook).
cp "$HERE/header_custom.html" "$WORK/_includes/"

cd "$HERE"
bundle exec jekyll build \
  --source "$WORK" \
  --destination "$OUT" \
  --config "$WORK/_config.yml,$HERE/_config_v8.yml"
