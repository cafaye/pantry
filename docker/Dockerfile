# kit template — Rust service image. Copied from kit/docker/Dockerfile.rust.
#
#   docker build -f docker/Dockerfile --build-arg RUST_VERSION=1.95 -t pantry .
#
# ONE DEVIATION FROM THE TEMPLATE, and it is load-bearing: the runtime stage
# copies `registry/` in. The template serves a binary that has everything it
# needs inside it; pantry's registry is repository data it reads at startup,
# which is the whole design — the registry is a reviewable directory of
# manifests, not something compiled into a binary and therefore not auditable in
# a diff. Everything else (two stages, musl, --locked, CA certificates,
# distroless, nonroot) is the template verbatim.
#
# STRICTNESS NOTES (inherited from the template)
#   - Two stages. The builder has rustc + cargo; the runtime does not.
#   - A fully static musl binary, so the runtime stage can be distroless/static.
#   - `--locked` is not optional: without it cargo silently updates Cargo.lock in
#     the build, so two builds of the same commit are not the same binary.
ARG RUST_VERSION=1.95

FROM rust:${RUST_VERSION}-slim-bookworm AS build
RUN rustup target add x86_64-unknown-linux-musl
WORKDIR /src
# Cargo resolves against the manifests first so the dependency layer caches.
COPY Cargo.toml Cargo.lock ./
RUN mkdir -p src \
    && echo 'fn main() {}' > src/main.rs \
    && cargo build --release --locked \
    && rm -rf src
COPY src ./src
# The vendored schema is compiled into the binary: /readyz validates every
# registry entry against it at startup, and it has to be the same bytes core
# publishes rather than a copy this image fetched at build time.
COPY schemas ./schemas
ARG SERVICE_NAME=pantry
RUN cargo build --release --locked --bin "${SERVICE_NAME}" \
    && cp "target/x86_64-unknown-linux-musl/release/${SERVICE_NAME}" /out/service \
    && strip "target/x86_64-unknown-linux-musl/release/${SERVICE_NAME}"

FROM gcr.io/distroless/static-debian12:nonroot AS runtime
WORKDIR /app
COPY --from=build /etc/ssl/certs/ca-certificates.crt /etc/ssl/certs/
COPY --from=build /out/service /app/service
# The registry. Read-only, and owned by nonroot so a compromised process cannot
# rewrite what it is about to answer with.
COPY --chown=nonroot:nonroot registry /etc/pantry/registry
ENV PANTRY_REGISTRY_DIR=/etc/pantry/registry
ENV PANTRY_BIND=0.0.0.0:8080
USER nonroot:nonroot
EXPOSE 8080
ENTRYPOINT ["/app/service"]
