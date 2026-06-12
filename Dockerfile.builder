# syntax=docker/dockerfile:1.10
# check=error=true;skip=UndefinedVar

# Override in CI for digest pinning:
#   docker build --build-arg BASE=debian:12-slim@sha256:<digest> ...
ARG BASE=debian:12-slim

# ─── shared base ──────────────────────────────────────────────────────────
FROM ${BASE} AS base
SHELL ["/bin/bash", "-eo", "pipefail", "-c"]
ENV DEBIAN_FRONTEND=noninteractive \
    LANG=C.UTF-8 \
    LC_ALL=C.UTF-8
RUN --mount=type=cache,target=/var/cache/apt,sharing=locked \
    --mount=type=cache,target=/var/lib/apt,sharing=locked <<EOF
rm -f /etc/apt/apt.conf.d/docker-clean
apt-get update -qq
apt-get install -y --no-install-recommends ca-certificates curl unzip xz-utils
EOF

# ─── CUDA toolkit (parallel) ─────────────────────────────────────────────
FROM base AS cuda
ARG CUDA_APT_VERSION=12-9
RUN --mount=type=cache,target=/var/cache/apt,sharing=locked \
    --mount=type=cache,target=/var/lib/apt,sharing=locked <<EOF
curl -fsSL -o /tmp/cuda-keyring.deb \
    https://developer.download.nvidia.com/compute/cuda/repos/debian12/x86_64/cuda-keyring_1.1-1_all.deb
dpkg -i /tmp/cuda-keyring.deb
rm /tmp/cuda-keyring.deb
apt-get update -qq
apt-get install -y --no-install-recommends \
    cuda-minimal-build-${CUDA_APT_VERSION} \
    libcudnn9-dev-cuda-12
EOF
RUN <<EOF
mkdir -p /tmp/cudnn-root
while IFS= read -r path; do
    if [[ -f "$path" || -L "$path" ]]; then
        mkdir -p "/tmp/cudnn-root$(dirname "$path")"
        cp -a "$path" "/tmp/cudnn-root$path"
    fi
done < <(dpkg -L libcudnn9-cuda-12 libcudnn9-headers-cuda-12 libcudnn9-dev-cuda-12 \
    | grep -E '^/(usr/include|usr/lib)')
EOF

# ─── Rust toolchain (parallel) ───────────────────────────────────────────
FROM base AS rust
ENV CARGO_HOME=/cache/cargo RUSTUP_HOME=/cache/rustup
ARG RUST_VERSION=1.88.0
ARG RUST_TARGETS="aarch64-linux-android"
RUN --mount=type=cache,target=/root/.cache <<EOF
curl --proto '=https' --tlsv1.2 -fsSL https://sh.rustup.rs \
  | sh -s -- -y --no-modify-path --profile minimal \
      --default-toolchain ${RUST_VERSION} \
      -c rustfmt -c clippy
for t in ${RUST_TARGETS}; do /cache/cargo/bin/rustup target add "$t"; done
rm -rf /cache/rustup/tmp /cache/cargo/registry
EOF

# ─── Bun (parallel) ──────────────────────────────────────────────────────
FROM base AS bun
ARG BUN_VERSION=1.3.12
RUN <<EOF
curl -fsSL https://bun.sh/install | bash -s "bun-v${BUN_VERSION}"
install -m 0755 /root/.bun/bin/bun /usr/local/bin/bun
EOF

# ─── Android SDK + NDK (parallel) ────────────────────────────────────────
FROM base AS android
ARG ANDROID_CMDLINE_VERSION=11076708
ARG ANDROID_CMDLINE_SHA256=
ARG ANDROID_PLATFORM=android-34
ARG ANDROID_PLATFORM_EXTRA=android-36
ARG ANDROID_BUILD_TOOLS=34.0.0
ARG ANDROID_BUILD_TOOLS_EXTRA=35.0.0
ARG ANDROID_NDK_VERSION=27.2.12479018
ENV ANDROID_HOME=/opt/android-sdk \
    JAVA_HOME=/usr/lib/jvm/java-17-openjdk-amd64 \
    NDK_HOME=/opt/android-sdk/ndk/27.2.12479018
RUN --mount=type=cache,target=/var/cache/apt,sharing=locked \
    --mount=type=cache,target=/var/lib/apt,sharing=locked <<EOF
apt-get update -qq
apt-get install -y --no-install-recommends openjdk-17-jdk-headless
EOF
# Cache the cmdline-tools zip across rebuilds; the SDK/NDK package downloads
# stay inside the (cached) android stage layer.
RUN --mount=type=cache,target=/var/cache/android-dl,sharing=locked <<EOF
mkdir -p "${ANDROID_HOME}/cmdline-tools"
ZIP=/var/cache/android-dl/cmdline-tools-${ANDROID_CMDLINE_VERSION}.zip
if [[ ! -f "$ZIP" ]]; then
  curl -fsSL -o "$ZIP" \
      "https://dl.google.com/android/repository/commandlinetools-linux-${ANDROID_CMDLINE_VERSION}_latest.zip"
fi
if [[ -n "${ANDROID_CMDLINE_SHA256}" ]]; then
  echo "${ANDROID_CMDLINE_SHA256}  $ZIP" | sha256sum -c -
fi
unzip -q "$ZIP" -d "${ANDROID_HOME}/cmdline-tools"
mv "${ANDROID_HOME}/cmdline-tools/cmdline-tools" "${ANDROID_HOME}/cmdline-tools/latest"
printf 'y\n%.0s' $(seq 1 50) | "${ANDROID_HOME}/cmdline-tools/latest/bin/sdkmanager" --licenses >/dev/null
"${ANDROID_HOME}/cmdline-tools/latest/bin/sdkmanager" \
    "platform-tools" \
    "platforms;${ANDROID_PLATFORM}" \
    "platforms;${ANDROID_PLATFORM_EXTRA}" \
    "build-tools;${ANDROID_BUILD_TOOLS}" \
    "build-tools;${ANDROID_BUILD_TOOLS_EXTRA}" \
    "ndk;${ANDROID_NDK_VERSION}" >/dev/null
rm -rf \
    "${NDK_HOME}/simpleperf" \
    "${NDK_HOME}/shader-tools" \
    "${NDK_HOME}/sources/third_party" \
    "${ANDROID_HOME}/emulator" \
    /root/.android
EOF

# ─── Final builder image ─────────────────────────────────────────────────
FROM base AS final
LABEL org.opencontainers.image.title="wt-builder" \
      org.opencontainers.image.description="WTranscriber release builder: Rust + Bun + Tauri Linux + Android SDK/NDK + CUDA" \
      org.opencontainers.image.source="https://github.com/lyntouch/wtranscriber"

ARG ANDROID_NDK_VERSION=27.2.12479018
ENV CARGO_HOME=/cache/cargo \
    RUSTUP_HOME=/cache/rustup \
    ANDROID_HOME=/opt/android-sdk \
    ANDROID_SDK_ROOT=/opt/android-sdk \
    NDK_HOME=/opt/android-sdk/ndk/${ANDROID_NDK_VERSION} \
    ANDROID_NDK=/opt/android-sdk/ndk/${ANDROID_NDK_VERSION} \
    ANDROID_NDK_ROOT=/opt/android-sdk/ndk/${ANDROID_NDK_VERSION} \
    ANDROID_NDK_HOME=/opt/android-sdk/ndk/${ANDROID_NDK_VERSION} \
    JAVA_HOME=/usr/lib/jvm/java-17-openjdk-amd64 \
    CUDA_HOME=/usr/local/cuda \
    CUDA_PATH=/usr/local/cuda \
    LD_LIBRARY_PATH=/usr/local/cuda/lib64 \
    PATH=/cache/cargo/bin:/usr/local/cuda/bin:/opt/android-sdk/cmdline-tools/latest/bin:/opt/android-sdk/platform-tools:/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin

# Native build deps for Tauri Linux + Rust crates with C bindings.
RUN --mount=type=cache,target=/var/cache/apt,sharing=locked \
    --mount=type=cache,target=/var/lib/apt,sharing=locked <<EOF
apt-get update -qq
apt-get install -y --no-install-recommends \
    build-essential \
    clang \
    cmake \
    file \
    git \
    libayatana-appindicator3-dev \
    libclang-dev \
    libgtk-3-dev \
    libjavascriptcoregtk-4.1-dev \
    librsvg2-dev \
    libsoup-3.0-dev \
    libssl-dev \
    libwebkit2gtk-4.1-dev \
    ninja-build \
    openjdk-17-jdk-headless \
    patchelf \
    pkg-config
EOF

COPY --from=cuda /usr/local/cuda-12.9 /usr/local/cuda-12.9
COPY --from=cuda /tmp/cudnn-root/ /
RUN ln -sfn /usr/local/cuda-12.9 /usr/local/cuda \
    && ldconfig

COPY --from=rust /cache/cargo /cache/cargo
COPY --from=rust /cache/rustup /cache/rustup
COPY --from=bun /usr/local/bin/bun /usr/local/bin/bun
COPY --from=android /opt/android-sdk /opt/android-sdk

RUN install -d -m 0777 /cache/cargo /cache/rustup /cache/target /work

# Login shells (bash -lc) re-source /etc/profile and would clobber PATH/env
# baked in via ENV. Drop a profile.d snippet so xtask's `bash -lc` invocations
# see the toolchain.
RUN cat >/etc/profile.d/wt-builder.sh <<'SH'
export CARGO_HOME=/cache/cargo
export RUSTUP_HOME=/cache/rustup
export ANDROID_HOME=/opt/android-sdk
export ANDROID_SDK_ROOT=/opt/android-sdk
export NDK_HOME=/opt/android-sdk/ndk/27.2.12479018
export ANDROID_NDK=$NDK_HOME
export ANDROID_NDK_ROOT=$NDK_HOME
export ANDROID_NDK_HOME=$NDK_HOME
export JAVA_HOME=/usr/lib/jvm/java-17-openjdk-amd64
export CUDA_HOME=/usr/local/cuda
export CUDA_PATH=/usr/local/cuda
export LD_LIBRARY_PATH=/usr/local/cuda/lib64${LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}
export PATH=/cache/cargo/bin:/usr/local/cuda/bin:/opt/android-sdk/cmdline-tools/latest/bin:/opt/android-sdk/platform-tools:$PATH
SH

WORKDIR /work
