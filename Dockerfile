# pantry — the Go service image.
#
#   docker build -t pantry .
#   docker build --build-arg SERVICE_NAME=pantry -t pantry .
#
# WHY THIS FILE IS AT THE REPOSITARY ROOT AND NOT NAMED `docker/Dockerfile.go`.
#
# Two reasons, and the first is the one that bites.
#
#   1. `go build ./...` compiles every `.go` file in the tree, and a file named
#      `docker/Dockerfile.go` is a Go source file to the go command. Naming a
#      Dockerfile `*.go` works in kit — a configuration repository with no
#      go.mod — and fails in any Go module. Measured, not assumed: `go build
#      ./...` printed `docker/Dockerfile.go:1:1: illegal character U+0023 '#'`.
#   2. identity's Go image is `./Dockerfile` at the root, and `docker/Dockerfile`
#      in this repository is the RUST image, which stays until pantry-02 gives
#      the Go service a read path. So the two images have two homes, each named
#      the way its own ecosystem expects.
#
# `docker/Dockerfile` (Rust) is deleted in the packet that deletes `src/`, not
# before.
#
# THE ARRANGEMENT is kit's Go template (kit/docker/Dockerfile.go), with the two
# deviations identity documents, and both are consequences of pantry not owning
# a database yet:
#
#   - DISTROLESS, not kit's debian-slim. kit's runtime is debian-slim because
#     the image starts through docker/entrypoint.sh, which migrates before it
#     serves, and a script needs a shell. There is no schema to migrate in this
#     packet, so there is no entrypoint to run, so the shell it needs is not
#     bought. kit's own file says the honest way back: set KIT_MIGRATE=off,
#     migrate from a job, and use distroless/static. This IS that, taken early
#     because the thing being avoided — a boot-time migration — does not exist
#     yet. pantry-02 re-opens it the moment it does.
#   - NO PROVENANCE STAMP. kit's template writes five labels from
#     docker/provenance.sh. Copying the script without the CI that feeds it five
#     build-args would stamp `unknown` on every image and make the labels a lie
#     that reads as data. Adopted with the Go migration wiring, not here.
#
# WHAT IS DELIBERATELY ABSENT: no `registry/` copy and no `schemas/` copy. Both
# are load-time data of the Rust implementation — it reads the manifests at
# startup and validates them against the vendored core schema. This binary has
# no read path at all (see internal/catalog), so copying either directory into the
# image would be copying data nothing reads, and the `CA2015`/`.dockerignore`
# conversation about it belongs to the packet that adds the reader.
#
# `--locked` is a cargo flag and has no Go equivalent; what makes two builds of
# one commit the same artefact here is `go mod download` against a committed
# go.sum with GOFLAGS unset, so a build cannot silently resolve a newer module.
ARG GO_VERSION=1.26

FROM golang:${GO_VERSION} AS build
WORKDIR /src
ENV CGO_ENABLED=0 GOOS=linux
# go.mod/go.sum first: the dependency layer caches until the manifests change.
COPY go.mod go.sum ./
RUN go mod download
COPY . .
ARG SERVICE_NAME=pantry
RUN go build -trimpath -ldflags="-s -w" -o /out/service ./cmd/${SERVICE_NAME}

FROM gcr.io/distroless/static-debian12:nonroot AS runtime
WORKDIR /app
COPY --from=build /out/service /app/service
# nonroot, not root. There is no shell in this image, so a compromised process
# cannot curl-and-pipe — the claim is measured in kit's own Dockerfile.go header,
# which shows `bin/` empty in both distroless variants.
USER nonroot:nonroot
EXPOSE 8080
# The service is stateless: no connection pool to close and nothing to flush, so
# SIGTERM is the whole shutdown story and cmd/pantry handles it.
ENTRYPOINT ["/app/service"]