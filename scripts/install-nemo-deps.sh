#!/usr/bin/env bash
# Install the NeMo Sortformer Python runtime under
# $WT_DATA_DIR/python (default ~/.local/share/wtranscriber/python).
#
# Strategy: bootstrap `uv` (standalone) into the same data dir if missing,
# then use `uv python install` + `uv pip install nemo_toolkit[asr]`.
# Output is line-buffered with simple `STEP:` / `PROGRESS:` markers so the
# Rust runtime installer can stream progress to the UI.

set -euo pipefail

WT_DATA_DIR="${WT_DATA_DIR:-$HOME/.local/share/wtranscriber}"
PY_VERSION="${WT_NEMO_PY:-3.12}"
RUNTIME_DIR="$WT_DATA_DIR/python"
UV_DIR="$WT_DATA_DIR/uv"
UV_BIN="$UV_DIR/uv"

mkdir -p "$WT_DATA_DIR" "$UV_DIR"

step() { printf 'STEP: %s\n' "$1"; }
fail() { printf 'ERROR: %s\n' "$1" >&2; exit 1; }

bootstrap_uv() {
    if [ -x "$UV_BIN" ]; then return 0; fi
    if command -v uv >/dev/null 2>&1; then
        UV_BIN="$(command -v uv)"
        return 0
    fi
    step "downloading uv"
    local arch
    arch="$(uname -m)"
    local triple
    case "$arch" in
        x86_64)  triple="x86_64-unknown-linux-gnu" ;;
        aarch64) triple="aarch64-unknown-linux-gnu" ;;
        *) fail "unsupported arch: $arch" ;;
    esac
    local url="https://github.com/astral-sh/uv/releases/latest/download/uv-${triple}.tar.gz"
    local tarball="$UV_DIR/uv.tar.gz"
    curl --fail --location --silent --show-error -o "$tarball" "$url" \
        || fail "failed to download uv from $url"
    tar -xzf "$tarball" -C "$UV_DIR" --strip-components=1
    rm -f "$tarball"
    [ -x "$UV_BIN" ] || fail "uv missing after extraction"
}

install_python() {
    if [ -x "$RUNTIME_DIR/bin/python3.12" ]; then
        step "python $PY_VERSION already installed"
        return 0
    fi
    step "installing python $PY_VERSION"
    local tmp_dir="$WT_DATA_DIR/python-tmp"
    rm -rf "$tmp_dir"
    "$UV_BIN" python install "$PY_VERSION" --install-dir "$tmp_dir" --reinstall
    local py_bin
    py_bin="$(find "$tmp_dir" -maxdepth 4 -path '*/bin/python3.12' -type f | head -1)"
    [ -n "$py_bin" ] || fail "could not locate python3.12 after install"
    local py_home
    py_home="$(dirname "$(dirname "$py_bin")")"
    rm -rf "$RUNTIME_DIR"
    mv "$py_home" "$RUNTIME_DIR"
    rm -rf "$tmp_dir"
    find "$RUNTIME_DIR" -name 'EXTERNALLY-MANAGED' -delete 2>/dev/null || true
}

install_nemo() {
    local py="$RUNTIME_DIR/bin/python3.12"
    [ -x "$py" ] || fail "python missing at $py"
    if "$py" -c 'import nemo.collections.asr' >/dev/null 2>&1; then
        step "nemo already installed"
        return 0
    fi
    step "installing nemo_toolkit[asr] (downloads torch + cuda wheels, may take a while)"
    "$UV_BIN" pip install --python "$py" --break-system-packages \
        --index-strategy unsafe-best-match \
        'nemo_toolkit[asr]'
    step "verifying nemo import"
    "$py" -c 'from nemo.collections.asr.models import SortformerEncLabelModel; print("sortformer OK")'
}

prune_runtime() {
    local sp
    sp="$("$RUNTIME_DIR/bin/python3.12" -c 'import site; print([p for p in site.getsitepackages() if "site-packages" in p][0])')"
    step "pruning $sp"
    find "$sp" -depth -type d \( -name __pycache__ -o -name tests -o -name test \) -exec rm -rf {} + 2>/dev/null || true
    find "$sp" -type f \( -name '*.pyc' -o -name '*.pyo' -o -name '*.a' \) -delete 2>/dev/null || true
    rm -rf "$sp/torch/include" 2>/dev/null || true
    rm -rf "$sp/triton" 2>/dev/null || true
}

bootstrap_uv
install_python
install_nemo
prune_runtime
step "done"
