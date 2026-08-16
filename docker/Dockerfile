# The Linux packages, built somewhere known.
#
# The base is what decides where the result will run: a binary linked against
# a glibc newer than the target's will not start there, and the README offers
# the .deb to Ubuntu 22.04, Mint 21 and Debian 12. Building on 22.04 (glibc
# 2.35) covers all of them and everything newer. Raise it only to drop them.
#
# If a base ever lacks libwebkit2gtk-4.1 — Tauri v2 wants the 4.1 API, not the
# 4.0 one — the build stops here rather than halfway through cargo.
ARG BASE=ubuntu:22.04
FROM ${BASE}

ARG NODE_MAJOR=22
ENV DEBIAN_FRONTEND=noninteractive \
    CARGO_HOME=/cargo \
    RUSTUP_HOME=/rustup \
    CARGO_TARGET_DIR=/target \
    PATH=/cargo/bin:$PATH

RUN apt-get update && apt-get install -y --no-install-recommends \
      build-essential ca-certificates curl file pkg-config git rsync \
      libwebkit2gtk-4.1-dev libgtk-3-dev libayatana-appindicator3-dev \
      librsvg2-dev libpcap-dev libssl-dev \
 && rm -rf /var/lib/apt/lists/*

# Node from NodeSource: the distribution's own is too old for Vite 5 on 22.04.
RUN curl -fsSL https://deb.nodesource.com/setup_${NODE_MAJOR}.x | bash - \
 && apt-get install -y --no-install-recommends nodejs \
 && rm -rf /var/lib/apt/lists/*

# Rust from rustup. Distribution Rust is 1.75 on 22.04 and against it the
# failure reads as missing methods rather than as an old compiler.
#
# Not pinned to the crate's own rust-version: that is a floor for *this* code,
# while the lock file's dependencies set their own and move it up without
# asking — pinning 1.87 here failed on a `time` that wanted 1.88. Pin a version
# only when a toolchain change actually breaks something, and then pin the one
# that works.
ARG RUST=stable
RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs \
      | sh -s -- -y --profile minimal --default-toolchain ${RUST} \
 && rustc --version

RUN npm install -g @tauri-apps/cli@^2

COPY build.sh /usr/local/bin/build.sh
RUN chmod +x /usr/local/bin/build.sh
ENTRYPOINT ["/usr/local/bin/build.sh"]
