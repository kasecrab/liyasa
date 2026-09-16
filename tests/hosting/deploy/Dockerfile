# The published image (HOST-02). Two stages so the shipped layer carries the
# binary and nothing that built it.
#
#   docker build -f tests/hosting/deploy/Dockerfile -t ghcr.io/liyasa/liyasa .
#
# The build context is the repository root.

FROM docker.io/library/rust:1.98-bookworm AS build
WORKDIR /src
# The manifests first, so a source-only change does not refetch the registry.
COPY Cargo.toml Cargo.lock ./
COPY crates ./crates
COPY xtask ./xtask
COPY tests ./tests
COPY schemas ./schemas
RUN cargo build --release --locked -p liyasa-server \
 && strip target/release/liyasa-server

FROM gcr.io/distroless/cc-debian12:nonroot
# Distroless: no shell, no package manager, and a non-root user by default.
# SQLite needs no server process, so the image is the binary and libc.
COPY --from=build /src/target/release/liyasa-server /usr/local/bin/liyasa-server

# The bundle and the database are mounted; nothing is baked in.
VOLUME ["/var/lib/liyasa"]
WORKDIR /var/lib/liyasa
EXPOSE 8080
USER nonroot:nonroot

ENV LIYASA_WORKER=container
# `--listen 0.0.0.0:8080` because a container's loopback is its own.
ENTRYPOINT ["/usr/local/bin/liyasa-server"]
CMD ["serve", "--listen", "0.0.0.0:8080", "--config", "/var/lib/liyasa", "--json-logs"]

HEALTHCHECK --interval=30s --timeout=3s --start-period=5s --retries=3 \
  CMD ["/usr/local/bin/liyasa-server", "jobs", "list", "--name", "__healthcheck"]
