# IronMUD container image, built from the prebuilt release tarballs (no Rust
# toolchain, no second build). The build context is a directory holding
# ironmud-<arch>-linux.tar.gz for each platform being built:
#
#   deploy/package-release.sh --out dist          # or download release assets into dist/
#   docker build -f deploy/docker/Dockerfile -t ironmud dist/
#
# Published images: ghcr.io/zombiecraig/ironmud-public:<tag> (amd64 + arm64).
#
# Run exactly ONE container per data volume: the database (sled) is
# single-writer, so never scale this to two replicas.

# Unpacking runs natively on the build host, even for the arm64 image.
FROM --platform=$BUILDPLATFORM ubuntu:24.04 AS unpack
ARG TARGETARCH
COPY ironmud-*-linux.tar.gz /dist/
RUN set -eu; \
    case "${TARGETARCH:-amd64}" in \
      amd64) arch=x86_64 ;; \
      arm64) arch=aarch64 ;; \
      *) echo "unsupported platform: $TARGETARCH" >&2; exit 1 ;; \
    esac; \
    mkdir /out; \
    tar -xzf "/dist/ironmud-${arch}-linux.tar.gz" -C /out --strip-components=1; \
    chmod -R u+rwX,go+rX,go-w /out

FROM ubuntu:24.04
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates tzdata \
    && rm -rf /var/lib/apt/lists/* \
    && useradd --system --uid 10001 --user-group --home-dir /opt/ironmud --shell /usr/sbin/nologin ironmud

# Program files are root-owned and read-only to the server; only the data
# directory is writable.
COPY --from=unpack /out/bin/ /opt/ironmud/bin/
COPY --from=unpack /out/scripts/ /opt/ironmud/scripts/
COPY --from=unpack /out/assets/ /opt/ironmud/assets/
COPY --from=unpack /out/VERSION /opt/ironmud/VERSION
RUN ln -s /opt/ironmud/bin/ironmud-admin /usr/local/bin/ironmud-admin \
    && ln -s /opt/ironmud/bin/ironmud-import /usr/local/bin/ironmud-import \
    && install -d -o ironmud -g ironmud -m 0750 /opt/ironmud/data

ENV IRONMUD_DATABASE=/opt/ironmud/data/ironmud.db \
    IRONMUD_BIND=0.0.0.0 \
    IRONMUD_OPS_BIND=127.0.0.1 \
    IRONMUD_OPS_PORT=4002

WORKDIR /opt/ironmud
USER ironmud
VOLUME ["/opt/ironmud/data"]
# 4000 = telnet. 4010 is the PROXY-protocol port, used only when
# IRONMUD_PROXY_PORT=4010 is set for a TLS proxy (Fly.io, HAProxy).
EXPOSE 4000

# First start seeds the world, which can take a minute on small hosts.
HEALTHCHECK --interval=30s --timeout=5s --start-period=120s --retries=3 \
    CMD ["ironmud-admin", "health", "-q"]

STOPSIGNAL SIGTERM
ENTRYPOINT ["/opt/ironmud/bin/ironmud"]
CMD ["--port", "4000"]
