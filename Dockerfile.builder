# syntax=docker/dockerfile:1.10
# check=error=true;skip=UndefinedVar

# Pin the base image by digest in CI for full reproducibility:
#   docker build --build-arg BASE=debian:12-slim@sha256:<digest> ...
ARG BASE=debian:12-slim
FROM ${BASE}

LABEL org.opencontainers.image.title="wt-builder" \
      org.opencontainers.image.description="WTranscriber release builder: Rust + Bun + Tauri Linux + Android SDK/NDK + CUDA" \
      org.opencontainers.image.source="https://github.com/lyntouch/wtranscriber"

SHELL ["/bin/bash", "-eo", "pipefail", "-c"]

ENV DEBIAN_FRONTEND=noninteractive \
    LANG=C.UTF-8 \
    LC_ALL=C.UTF-8 \
    CARGO_HOME=/cache/cargo \
    RUSTUP_HOME=/cache/rustup \
    ANDROID_HOME=/opt/android-sdk \
    ANDROID_SDK_ROOT=/opt/android-sdk \
    JAVA_HOME=/usr/lib/jvm/java-17-openjdk-amd64 \
    CUDA_HOME=/usr/local/cuda \
    CUDA_PATH=/usr/local/cuda \
    LD_LIBRARY_PATH=/usr/local/cuda/lib64 \
    PATH=/cache/cargo/bin:/usr/local/cuda/bin:/opt/android-sdk/cmdline-tools/latest/bin:/opt/android-sdk/platform-tools:/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin

RUN --mount=type=cache,target=/var/cache/apt,sharing=locked \
    --mount=type=cache,target=/var/lib/apt,sharing=locked <<'EOF'
rm -f /etc/apt/apt.conf.d/docker-clean
apt-get update -qq
apt-get install -y --no-install-recommends \
    build-essential \
    ca-certificates \
    clang \
    cmake \
    curl \
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
    pkg-config \
    unzip \
    xz-utils
EOF

ARG CUDA_APT_VERSION=12-9
ARG CUDA_VERSION=12.9
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
ln -sfn /usr/local/cuda-${CUDA_VERSION} /usr/local/cuda
EOF

ARG RUST_VERSION=1.88.0
ARG RUST_TARGETS="aarch64-linux-android"
RUN --mount=type=cache,target=/cache/cargo/registry,sharing=locked <<EOF
curl --proto '=https' --tlsv1.2 -fsSL https://sh.rustup.rs \
  | sh -s -- -y --no-modify-path --profile minimal \
      --default-toolchain ${RUST_VERSION} \
      -c rustfmt -c clippy
for t in ${RUST_TARGETS}; do /cache/cargo/bin/rustup target add "$t"; done
rustc --version
rm -rf /cache/rustup/tmp
EOF

ARG BUN_VERSION=1.3.12
RUN <<EOF
curl -fsSL https://bun.sh/install | bash -s "bun-v${BUN_VERSION}"
install -m 0755 /root/.bun/bin/bun /usr/local/bin/bun
rm -rf /root/.bun
bun --version
EOF

ARG ANDROID_CMDLINE_VERSION=11076708
ARG ANDROID_PLATFORM=android-34
ARG ANDROID_BUILD_TOOLS=34.0.0
ARG ANDROID_NDK_VERSION=27.2.12479018
# Pin to a known sha256 in CI by setting --build-arg ANDROID_CMDLINE_SHA256=<hash>;
# Google publishes hashes at https://developer.android.com/studio#command-line-tools-only
ARG ANDROID_CMDLINE_SHA256=
ENV NDK_HOME=${ANDROID_HOME}/ndk/${ANDROID_NDK_VERSION} \
    ANDROID_NDK=${ANDROID_HOME}/ndk/${ANDROID_NDK_VERSION} \
    ANDROID_NDK_ROOT=${ANDROID_HOME}/ndk/${ANDROID_NDK_VERSION} \
    ANDROID_NDK_HOME=${ANDROID_HOME}/ndk/${ANDROID_NDK_VERSION}
RUN <<EOF
mkdir -p "${ANDROID_HOME}/cmdline-tools"
curl -fsSL -o /tmp/cmdline-tools.zip \
    "https://dl.google.com/android/repository/commandlinetools-linux-${ANDROID_CMDLINE_VERSION}_latest.zip"
if [[ -n "${ANDROID_CMDLINE_SHA256}" ]]; then
  echo "${ANDROID_CMDLINE_SHA256}  /tmp/cmdline-tools.zip" | sha256sum -c -
fi
unzip -q /tmp/cmdline-tools.zip -d "${ANDROID_HOME}/cmdline-tools"
mv "${ANDROID_HOME}/cmdline-tools/cmdline-tools" "${ANDROID_HOME}/cmdline-tools/latest"
rm /tmp/cmdline-tools.zip
yes | "${ANDROID_HOME}/cmdline-tools/latest/bin/sdkmanager" --licenses >/dev/null
"${ANDROID_HOME}/cmdline-tools/latest/bin/sdkmanager" \
    "platform-tools" \
    "platforms;${ANDROID_PLATFORM}" \
    "build-tools;${ANDROID_BUILD_TOOLS}" \
    "ndk;${ANDROID_NDK_VERSION}" >/dev/null
rm -rf \
    "${NDK_HOME}/simpleperf" \
    "${NDK_HOME}/shader-tools" \
    "${NDK_HOME}/sources/third_party" \
    "${ANDROID_HOME}/emulator" \
    /root/.android \
    /tmp/*
EOF

RUN install -d -m 0777 /cache/cargo /cache/rustup /cache/target /work
WORKDIR /work
