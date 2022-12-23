# Build
FROM registry.gitlab.com/rust_musl_docker/image:stable-1.65.0@sha256:f95635740757e0e9a29b51d5060a7564e900cb5192defdd3a5b2ba5b47ce17b3 as build

COPY . /workdir

RUN rustup target add x86_64-unknown-linux-musl

WORKDIR /workdir/radicle-httpd
RUN cp gitconfig /etc/gitconfig
RUN RUSTFLAGS=-Cforce-frame-pointers=yes cargo build --profile=container --target=x86_64-unknown-linux-musl
RUN objcopy --compress-debug-sections /workdir/target/x86_64-unknown-linux-musl/container/radicle-httpd /workdir/target/x86_64-unknown-linux-musl/container/radicle-httpd.compressed

# Run
FROM debian:bullseye-slim@sha256:25f10b4f1ded5341a3ca0a30290ff3cd5639415f0c5a2222d5e7d5dd72952aa1

RUN echo deb http://deb.debian.org/debian bullseye-backports main contrib non-free >/etc/apt/sources.list.d/backports.list
RUN apt-get update && apt -t bullseye-backports install --yes git && rm -rf /var/lib/apt/lists/*
COPY --from=build /workdir/target/x86_64-unknown-linux-musl/container/radicle-httpd.compressed /usr/local/bin/radicle-httpd
COPY --from=build /etc/gitconfig /etc/gitconfig

WORKDIR /app

ENTRYPOINT ["/usr/local/bin/radicle-httpd", "--listen", "0.0.0.0:8080"]
