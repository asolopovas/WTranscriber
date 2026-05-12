FROM debian:12-slim

ENV DEBIAN_FRONTEND=noninteractive \
    CARGO_HOME=/cache/cargo \
    RUSTUP_HOME=/cache/rustup \
    ANDROID_HOME=/opt/android-sdk \
    ANDROID_SDK_ROOT=/opt/android-sdk \
    NDK_HOME=/opt/android-sdk/ndk/27.2.12479018 \
    ANDROID_NDK=/opt/android-sdk/ndk/27.2.12479018 \
    ANDROID_NDK_ROOT=/opt/android-sdk/ndk/27.2.12479018 \
    ANDROID_NDK_HOME=/opt/android-sdk/ndk/27.2.12479018 \
    JAVA_HOME=/usr/lib/jvm/default-java \
    PATH=/cache/cargo/bin:/opt/android-sdk/cmdline-tools/latest/bin:/opt/android-sdk/platform-tools:/usr/local/bin:/usr/bin:/bin

RUN apt-get update -qq && apt-get install -y --no-install-recommends \
        build-essential \
        ca-certificates \
        clang \
        cmake \
        curl \
        default-jdk-headless \
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
        patchelf \
        pkg-config \
        unzip \
        wget \
        xz-utils \
    && rm -rf /var/lib/apt/lists/*

ARG RUST_VERSION=1.88.0
RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs \
        | sh -s -- -y --profile minimal --default-toolchain ${RUST_VERSION} \
            -c rustfmt -c clippy \
    && /cache/cargo/bin/rustup target add \
        aarch64-linux-android \
        armv7-linux-androideabi \
        i686-linux-android \
        x86_64-linux-android

ARG BUN_VERSION=1.3.12
RUN curl -fsSL https://bun.sh/install | bash -s "bun-v${BUN_VERSION}" \
    && mv /root/.bun/bin/bun /usr/local/bin/bun \
    && bun --version

ARG ANDROID_CMDLINE_VERSION=11076708
ARG ANDROID_PLATFORM=android-34
ARG ANDROID_BUILD_TOOLS=34.0.0
ARG ANDROID_NDK_VERSION=27.2.12479018
RUN mkdir -p ${ANDROID_HOME}/cmdline-tools \
    && cd /tmp \
    && curl -fsSL -o cmdline-tools.zip \
        https://dl.google.com/android/repository/commandlinetools-linux-${ANDROID_CMDLINE_VERSION}_latest.zip \
    && unzip -q cmdline-tools.zip -d ${ANDROID_HOME}/cmdline-tools \
    && mv ${ANDROID_HOME}/cmdline-tools/cmdline-tools ${ANDROID_HOME}/cmdline-tools/latest \
    && rm cmdline-tools.zip \
    && yes | ${ANDROID_HOME}/cmdline-tools/latest/bin/sdkmanager --licenses >/dev/null \
    && ${ANDROID_HOME}/cmdline-tools/latest/bin/sdkmanager \
        "platform-tools" \
        "platforms;${ANDROID_PLATFORM}" \
        "build-tools;${ANDROID_BUILD_TOOLS}" \
        "ndk;${ANDROID_NDK_VERSION}" >/dev/null

RUN install -d -m 0777 /cache/cargo /cache/rustup /cache/target /work
WORKDIR /work
