#!/usr/bin/env bash
# Renders the demo GIFs with vhs, then cuts them down to 8 frames per second and a
# 48-colour palette: a quarter smaller, and the level colours still tell apart.
#
#   docs/demos/render.sh            every tape
#   docs/demos/render.sh s3         the tapes whose names start with s3
set -euo pipefail

cd "$(dirname "$0")/../.."
cargo build --release --quiet

for tape in docs/demos/${1:-s[0-9]}*.tape; do
    gif="${tape%.tape}.gif"
    vhs --quiet "$tape"
    ffmpeg -loglevel error -y -i "$gif" -vf \
        "fps=8,split[a][b];[a]palettegen=max_colors=48[p];[b][p]paletteuse=dither=none:diff_mode=rectangle" \
        "$gif.tmp.gif"
    mv "$gif.tmp.gif" "$gif"
    echo "$gif $(du -h "$gif" | cut -f1)"
done
