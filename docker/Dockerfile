# syntax=docker/dockerfile:1

# BASE: everything up to here runs on the build host's architecture. Shared by
# `spec` and `builder`; contains nothing that depends on the target platform.
FROM --platform=$BUILDPLATFORM rust:1.99-slim-bookworm@sha256:f2c004e3d2d8b547e8cf7ffbc29205385f4e71d23e53e4e383065db5e9f822cd AS base

ENV SQLX_OFFLINE=true

WORKDIR /app

RUN apt-get update \
    && apt-get install -y --no-install-recommends git pkg-config \
    && rm -rf /var/lib/apt/lists/*

ARG SWAGGER_UI="https://drive.usercontent.google.com/download?id=1GHIMYle518Qy8aB-AuqasCUYSKN_XRpw&export=download"
# is downloaded through the image builder with ADD below, utoipa skips download
# if this is locally available
ENV SWAGGER_UI_DOWNLOAD_URL=file:///app/swagger-ui.zip
ADD --chmod=0644 ${SWAGGER_UI} /app/swagger-ui.zip

# SOURCES: the build context subset both `spec` and `builder` compile
FROM scratch AS sources

COPY Cargo.toml Cargo.lock /
COPY ./.sqlx /.sqlx
COPY ./migrations /migrations
COPY ./pazufa-backend-lib /pazufa-backend-lib
COPY ./track-validation /track-validation
COPY ./pazufa-backend /pazufa-backend
COPY ./pazufa-models /pazufa-models
COPY ./elastic-companion /elastic-companion

# SPEC: openapi.yml is the same for every platform, so it is generated once from a
# native (host-arch) debug build instead of running the cross-compiled binary.
# Nothing here depends on TARGETARCH, so multi-platform builds share this stage.
FROM base AS spec

RUN apt-get update \
    && apt-get install -y --no-install-recommends jq yq \
    && rm -rf /var/lib/apt/lists/*

COPY --from=sources / ./

# .git is only needed by pazufa-backend-lib/build.rs (`git rev-parse HEAD`)
RUN --mount=type=bind,source=.git,target=.git \
    nice cargo build --locked --bin pazufa-backend \
    && target/debug/pazufa-backend --dump-spec | jq -S "." | yq -y > /openapi.yml

# BUILDER: cross-compiles the release binaries for TARGETARCH on the build host,
# so multi-platform builds never run under QEMU emulation.
FROM base AS builder

ARG TARGETARCH

# Cross toolchain + target-arch OpenSSL (Debian multiarch); writes the cargo/cc/openssl
# environment for the target to /etc/cross.env, which the build step sources.
# openssl-sys accepts a single include dir, but Debian splits the headers into
# /usr/include/openssl and the arch-specific /usr/include/<triplet>/openssl (configuration.h),
# so both are merged into /opt/openssl/include via symlinks.
RUN case "$TARGETARCH" in \
        amd64) gnu=x86_64-linux-gnu  rust=x86_64-unknown-linux-gnu ;; \
        arm64) gnu=aarch64-linux-gnu rust=aarch64-unknown-linux-gnu ;; \
        *) echo "unsupported TARGETARCH: $TARGETARCH" >&2; exit 1 ;; \
    esac \
    && cross="" \
    && if [ "$(dpkg --print-architecture)" != "$TARGETARCH" ]; then \
        dpkg --add-architecture "$TARGETARCH"; \
        cross="gcc-$(echo "$gnu" | tr _ -) libc6-dev-$TARGETARCH-cross"; \
    fi \
    && apt-get update \
    && apt-get install -y --no-install-recommends "libssl-dev:$TARGETARCH" $cross \
    && rm -rf /var/lib/apt/lists/* \
    && rustup target add "$rust" \
    && mkdir -p /opt/openssl/include/openssl \
    && ln -s /usr/include/openssl/* /opt/openssl/include/openssl/ \
    && ln -sf "/usr/include/$gnu/openssl/"* /opt/openssl/include/openssl/ \
    && test -e /opt/openssl/include/openssl/configuration.h \
    && target_env="$(echo "$rust" | tr a-z- A-Z_)" \
    && { \
        echo "export RUST_TARGET=$rust"; \
        echo "export CARGO_TARGET_${target_env}_LINKER=$gnu-gcc"; \
        echo "export CC_$(echo "$rust" | tr - _)=$gnu-gcc"; \
        echo "export ${target_env}_OPENSSL_LIB_DIR=/usr/lib/$gnu"; \
        echo "export ${target_env}_OPENSSL_INCLUDE_DIR=/opt/openssl/include"; \
        echo "export PKG_CONFIG_ALLOW_CROSS=1"; \
        echo "export PKG_CONFIG_LIBDIR=/usr/lib/$gnu/pkgconfig:/usr/share/pkgconfig"; \
    } > /etc/cross.env

# The only writable directories of the runners, owned by distroless' `nonroot` user (65532).
# Everything else (incl. /app) stays root-owned and read-only for the app.
# Distroless has no shell, so they are prepared here and copied over.
# /app must exist before the runners' `COPY --chmod ... /app/`: otherwise BuildKit
# creates it with that --chmod mode (e.g. 0444, no search bit) and nothing in it is reachable.
RUN mkdir -p /rootfs/backend/var/log/pazufa-backend /rootfs/ec/var/lib/elastic-companion \
             /rootfs/backend/app /rootfs/ec/app \
    && chown 65532:65532 /rootfs/backend/var/log/pazufa-backend /rootfs/ec/var/lib/elastic-companion \
    && chmod 0700 /rootfs/backend/var/log/pazufa-backend /rootfs/ec/var/lib/elastic-companion \
    && chmod 0555 /rootfs/backend/app /rootfs/ec/app

ARG TRACK_PATH="https://codeberg.org/PaZuFa/parlamentszusammenfasser/raw/tag/v0.2.9+v0.1.4/tracks.toml"
ADD --chmod=0644 ${TRACK_PATH} /artifacts/tracks.toml

COPY --from=sources / ./

# Binaries that the deploy pipeline attaches to the latest Codeberg release
ARG RELEASE_ARTIFACTS="elastic-companion pazufa-backend prefixmatch validate-vorgang"
# .git is only needed by pazufa-backend-lib/build.rs (`git rev-parse HEAD`)
RUN --mount=type=bind,source=.git,target=.git \
    . /etc/cross.env \
    && nice cargo build --locked --release --target "$RUST_TARGET" \
       $(printf '%s\n' $RELEASE_ARTIFACTS | sort -u | sed 's/^/--bin /') \
    && for bin in $RELEASE_ARTIFACTS; do cp "target/$RUST_TARGET/release/$bin" /artifacts/; done

COPY --from=spec /openapi.yml /artifacts/openapi.yml

# PAZUFA BACKEND RUNNER
# distroless/cc ships glibc, libgcc_s, libssl3 and CA certificates; no shell, no package manager
FROM gcr.io/distroless/cc-debian12:nonroot@sha256:9dac0a79194e45a7da0158a9c6da57b217585af0786db3845d1f0ec1a0dd182f AS pazufa-backend-runner

LABEL org.opencontainers.image.authors="Benedikt Schäfer" \
      org.opencontainers.image.description="Backend for the PaZuFa"
# backend-label
LABEL org.opencontainers.image.version="0.3.6"

ENV RUST_LOG=warn,pazufa_backend=info \
    PAZUFA_PORT=8080 \
    PAZUFA_ERROR_LOG=/var/log/pazufa-backend/error.log

COPY --from=builder /rootfs/backend/ /
COPY --from=builder --chmod=0444 /artifacts/openapi.yml /artifacts/tracks.toml /app/
COPY --from=builder --chmod=0555 /artifacts/pazufa-backend /app/

WORKDIR /app

USER 65532:65532

ENTRYPOINT ["./pazufa-backend"]

# ELASTIC COMPANION RUNNER
FROM gcr.io/distroless/cc-debian12:nonroot@sha256:9dac0a79194e45a7da0158a9c6da57b217585af0786db3845d1f0ec1a0dd182f AS elastic-companion-runner

LABEL org.opencontainers.image.authors="Benedikt Schäfer" \
      org.opencontainers.image.description="Elastic Companion Application to PaZuFa Backend"
# ec-label
LABEL org.opencontainers.image.version="0.1.0"

ENV RUST_LOG=warn,elastic_companion=info

COPY --from=builder /rootfs/ec/ /
COPY --from=builder --chmod=0444 /artifacts/tracks.toml       /app/
COPY --from=builder --chmod=0555 /artifacts/elastic-companion /app/

WORKDIR /app

USER 65532:65532

ENTRYPOINT ["./elastic-companion", "--iv", "vorgang_test_index", "--id", "dokument_test_index", "--is", "sitzung_test_index", "--timestamp-store", "/var/lib/elastic-companion/last_es_sync"]

# RELEASE ARTIFACTS
# Not meant to be run: exported with `--output type=local` by the deploy pipeline
FROM scratch AS release-artifacts

COPY --from=builder /artifacts/ /
