#!/usr/bin/env bash
set -euo pipefail

# Build the WTranscriber .deb inside a Debian 12 container so the resulting
# binary requires glibc <= 2.36 and runs on Debian 12+, Ubuntu 22.04+ (with
# webkit backport) and Ubuntu 24.04+. See docs/release.md.

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
IMAGE="${WT_DEB_IMAGE:-wt-deb-builder:debian12}"
CACHE_VOL_CARGO="${WT_DEB_CARGO_VOL:-wt-deb-cargo}"
CACHE_VOL_TARGET="${WT_DEB_TARGET_VOL:-wt-deb-target}"
CACHE_VOL_BUN="${WT_DEB_BUN_VOL:-wt-deb-bun}"

cd "$ROOT"

if ! docker image inspect "$IMAGE" >/dev/null 2>&1 || [[ "${WT_DEB_REBUILD:-0}" == "1" ]]; then
    echo "[deb-docker] building image $IMAGE"
    docker build -f docker/Dockerfile.deb -t "$IMAGE" docker/
fi

for vol in "$CACHE_VOL_CARGO" "$CACHE_VOL_TARGET" "$CACHE_VOL_BUN"; do
    docker volume inspect "$vol" >/dev/null 2>&1 || docker volume create "$vol" >/dev/null
done

UID_GID="$(id -u):$(id -g)"

docker run --rm -i \
    -v "$ROOT:/work" \
    -v "$CACHE_VOL_CARGO:/cache/cargo" \
    -v "$CACHE_VOL_TARGET:/cache/target" \
    -v "$CACHE_VOL_BUN:/cache/bun" \
    -e CARGO_TARGET_DIR=/cache/target \
    -e CARGO_INCREMENTAL=1 \
    -e BUN_INSTALL_CACHE_DIR=/cache/bun \
    -w /work \
    "$IMAGE" bash -lc '
        set -euo pipefail
        unset SHERPA_ONNX_LIB_DIR SHERPA_ONNX_LIB SHERPA_ONNX_INCLUDE_DIR || true
        bun install --frozen-lockfile --no-progress 2>&1 | tail -5
        bun run tauri build --bundles deb -- --no-default-features --features sherpa-static
        SRC="/cache/target/release/bundle/deb"
        DST="/work/src-tauri/target/release/bundle/deb"
        mkdir -p "$DST"
        cp -f "$SRC"/*.deb "$DST"/
        chown -R '"$UID_GID"' "$DST" /work/dist /work/node_modules 2>/dev/null || true
    '

OUT="src-tauri/target/release/bundle/deb"
echo "[deb-docker] artefacts:"
ls -lh "$OUT"/*.deb 2>/dev/null || { echo "  (no .deb produced)"; exit 1; }
