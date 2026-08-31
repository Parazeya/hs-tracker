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

# The AppImage half, in a layer of its own so adding it does not invalidate the
# toolchain above — those take minutes, this takes seconds.
#
# linuxdeploy needs patchelf, zsync and the desktop-file utils; its GTK plugin
# copies in what the webview uses to play the alert sounds; its own runtime
# wants FUSE 2, which is why the bundler is also run with
# APPIMAGE_EXTRACT_AND_RUN so it never has to mount anything. Only `--appimage`
# reaches any of it, so a plain `npm run deb` is unaffected.
#
# squashfs-tools is for trim-appimage.sh, which unpacks the finished AppImage,
# drops the libraries that have to come from the host, and packs it up again
# with the runtime the bundler already downloaded.
RUN apt-get update && apt-get install -y --no-install-recommends \
      patchelf zsync desktop-file-utils fuse3 libfuse2 squashfs-tools \
      gstreamer1.0-plugins-base gstreamer1.0-plugins-good gstreamer1.0-libav \
 && rm -rf /var/lib/apt/lists/*
ENV APPIMAGE_EXTRACT_AND_RUN=1

# What this image is fit to produce. Read by build.sh, which refuses anything
# else — an .rpm bundled here installs on Fedora and then cannot start, because
# the binary asks for the libpcap Debian ships and Fedora does not have. See
# Dockerfile.fedora.
ENV PKG_FAMILY=deb

# Git on Windows hands this over with CRLF line endings, and a carriage
# return on the shebang line makes the entrypoint "bash\r: No such file
# or directory" - a 127 with no other clue. Strip them here so the checkout
# settings of whoever builds the image stop mattering.
COPY build.sh trim-appimage.sh /usr/local/bin/
RUN sed -i 's/\r$//' /usr/local/bin/build.sh /usr/local/bin/trim-appimage.sh \
 && chmod +x /usr/local/bin/build.sh /usr/local/bin/trim-appimage.sh
ENTRYPOINT ["/usr/local/bin/build.sh"]
