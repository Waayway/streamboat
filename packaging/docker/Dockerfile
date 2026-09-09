# syntax=docker/dockerfile:1
#
# streamboatd -- the headless daemon, packaged as a container image (D-041).
#
# Multi-stage: build with the full GStreamer *development* packages on
# rust:bookworm, run with only the GStreamer *runtime* plugins streamboatd
# actually needs on debian:bookworm-slim (D-020's plugin set: base/good/bad
# for the DASH/fMP4 demux path, libav for the LGPL avdec_aac decoder --
# never fdk-aac -- and alsa for the exclusive-mode writer's underlying
# device). No GUI toolkit anywhere in this image -- see the
# `headless-daemon` job in .github/workflows/ci.yml, which asserts the same
# thing about the binary itself.
#
# Build from the repository root:
#   docker build -f packaging/docker/Dockerfile -t streamboat-daemon .
#
# Run (see packaging/docker/compose.yml for the same shape as a compose
# service):
#   docker run --rm -it \
#     --device /dev/snd \
#     --group-add "$(getent group audio | cut -d: -f3)" \
#     -p 127.0.0.1:4747:4747 \
#     -v streamboat-data:/var/lib/streamboat \
#     streamboat-daemon
#
# streamboatd's control API binds 127.0.0.1:4747 *inside the container* by
# default -- publishing that port with `-p 127.0.0.1:4747:4747` keeps it
# reachable only from the host, matching the daemon's own loopback-by-default
# policy (docs/architecture.md, "Control API"). To expose it on the LAN
# instead, pass `--lan` (or `--listen 0.0.0.0:4747`) as an extra argument to
# `docker run`/`command:` *and* change the port mapping to bind a non-loopback
# host address -- both steps are deliberate, matching the daemon's own
# "logs a warning naming the bind address" behaviour when it is told to
# listen beyond localhost.

# ---- Build stage ----------------------------------------------------------
FROM rust:bookworm AS build

RUN apt-get update && apt-get install -y --no-install-recommends \
        pkg-config \
        libgstreamer1.0-dev \
        libgstreamer-plugins-base1.0-dev \
        libgstreamer-plugins-bad1.0-dev \
        libasound2-dev \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /src
COPY . .

RUN cargo build --release --locked -p streamboat-server

# ---- Runtime stage ----------------------------------------------------------
FROM debian:bookworm-slim AS runtime

RUN apt-get update && apt-get install -y --no-install-recommends \
        libgstreamer1.0-0 \
        gstreamer1.0-plugins-base \
        gstreamer1.0-plugins-good \
        gstreamer1.0-plugins-bad \
        gstreamer1.0-libav \
        gstreamer1.0-alsa \
        libasound2 \
        ca-certificates \
    && rm -rf /var/lib/apt/lists/*

# Dedicated, non-root user. Its numeric GID is not fixed to the host's
# `audio` GID -- `docker run --group-add "$(getent group audio ...)"` (or
# the compose.yml `group_add:` list) adds the *host's* audio group to this
# container's supplementary groups at run time instead, the same
# `--device /dev/snd` + `--group-add` pattern the headless-and-tidal-connect
# daemon-architecture reference documents for ALSA-in-Docker.
RUN groupadd --system streamboat \
    && useradd --system --gid streamboat --home-dir /var/lib/streamboat --create-home streamboat

COPY --from=build /src/target/release/streamboatd /usr/local/bin/streamboatd

ENV STREAMBOAT_HOME=/var/lib/streamboat
VOLUME ["/var/lib/streamboat"]

USER streamboat
WORKDIR /var/lib/streamboat

# streamboatd has no interactive login flow of its own inside the container;
# run `streamboat login` once against the same STREAMBOAT_HOME/volume (or
# any other client using the same data directory) before expecting playback
# to work. See README.md's "Client credentials" and "Logging in" sections.

EXPOSE 4747
ENTRYPOINT ["/usr/local/bin/streamboatd"]
CMD []
