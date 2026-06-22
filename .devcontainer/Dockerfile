FROM mcr.microsoft.com/devcontainers/rust:1

# Tauri's Linux build dependencies. Baked into an image layer so it's cached
# across rebuilds (a postCreateCommand would re-run apt every time instead).
# BuildKit cache mounts keep the downloaded .deb files around, so editing this
# package list later only fetches the diff.
RUN --mount=type=cache,target=/var/cache/apt,sharing=locked \
    --mount=type=cache,target=/var/lib/apt,sharing=locked \
    rm -f /etc/apt/apt.conf.d/docker-clean \
    && apt-get update \
    && DEBIAN_FRONTEND=noninteractive apt-get install -y --no-install-recommends \
        build-essential \
        curl \
        wget \
        file \
        libssl-dev \
        libgtk-3-dev \
        libayatana-appindicator3-dev \
        librsvg2-dev \
        libwebkit2gtk-4.1-dev \
        libsoup-3.0-dev
