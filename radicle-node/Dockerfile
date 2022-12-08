# Build
FROM rust:1.65.0-slim@sha256:27349440d16ac15f52e8cae6181999a1b2fc05272d612a358d0e53078fcef88e as build

RUN apt-get update && apt-get install -y pkg-config libssl-dev git cmake

COPY . /app
WORKDIR /app/radicle-node

RUN set -eux; \
    cargo install --locked --path .; \
    objcopy --compress-debug-sections /usr/local/cargo/bin/radicle-node /usr/local/cargo/bin/radicle-node.compressed

# Run
FROM debian:bullseye-slim@sha256:25f10b4f1ded5341a3ca0a30290ff3cd5639415f0c5a2222d5e7d5dd72952aa1

RUN echo deb http://deb.debian.org/debian bullseye-backports main contrib non-free >/etc/apt/sources.list.d/backports.list
RUN apt-get update && apt-get install -y libssl1.1 && apt -t bullseye-backports install --yes git && rm -rf /var/lib/apt/lists/*
COPY --from=build /usr/local/cargo/bin/radicle-node.compressed /usr/local/bin/radicle-node

WORKDIR /app

ENTRYPOINT ["/usr/local/bin/radicle-node"]
