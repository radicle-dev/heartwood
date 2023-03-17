# ❤️🪵

*Radicle Heartwood Protocol & Stack*

Heartwood is the third iteration of the Radicle Protocol, a powerful
peer-to-peer code collaboration and publishing stack. The repository contains a
full implemention of Heartwood, complete with a user-friendly command-line
interface (`rad`) and network daemon (`radicle-node`).

Radicle was designed to be a secure, decentralized and powerful alternative to
code forges such as GitHub and GitLab that preserves user sovereignty
and freedom.

## Installation

**Requirements**

* *Linux* or *Unix* based operating system.
* Git 2.34 or later
* OpenSSH 9.1 or later with `ssh-agent`

### 📦 From source

> Requires the Rust toolchain.

You can install the Radicle stack from source, by running the following
commands from inside this repository:

    cargo install --path radicle-cli --force --locked
    cargo install --path radicle-node --force --locked
    cargo install --path radicle-remote-helper --force --locked

Or directly from our seed node:

    cargo install --force --locked --git https://seed.radicle.xyz/z3gqcJUoA1n9HaHKufZs5FCSGazv5.git \
        radicle-cli radicle-node radicle-remote-helper

### With Docker

In case you want to run radicle in containers, on the same host (e.g. your laptop),
see [RUNNING_IN_CONTAINERS.md](RUNNING_IN_CONTAINERS.md)

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md) and [HACKING.md](HACKING.md) for an
introduction to contributing to Radicle.

## License

Radicle is distributed under the terms of both the MIT license and the Apache License (Version 2.0).

See [LICENSE-APACHE](LICENSE-APACHE) and [LICENSE-MIT](LICENSE-MIT) for details.
