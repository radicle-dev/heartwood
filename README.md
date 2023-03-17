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
you can use the `docker-compose.yml` file provided within this repo. 

For the first node you want to start on your machine, you can: 

#### Create a profile 
1. Create a folder where you will store the data of your node. e.g. `mkdir -p ~/radicle/profiles/bob/.radicle`
1. Set `RAD_HOME` : `export RAD_HOME=~/radicle/profiles/bob/.radicle`
1. Create a key:
   - Pick a good passphrase and store it in your password manager
   - go ahead with creating the key `rad auth --stdin` (or use `RAD_PASSPHRASE` env var)
   - your profile should be created in `~/radicle/profiles/bob/.radicle`. 

#### Build the container images

This takes a couple of minutes - depending on your machine - as it needs 
to download the parent container images, and also all the rust 
dependencies and then compile the code: 

```bash
docker-compose build
```

#### Run First Node 
1. Create a `.env.$nodename` file that will store all your environment variables:
```yaml
# these options are especially useful in a development setting - probably not for production use
RADICLE_NODE_OPTIONS=--tracking-policy track --tracking-scope all
# Note the difference between RAD_ROOT vs. RAD_HOME.
RAD_ROOT=~/radicle/profiles/bob
# ensure these ports are free on your machine
RADICLE_API_PORT=8888
RADICLE_NODE_PORT=8778
```
1. Start the containers:
```bash
# we don't need to start the included `caddy` service
docker-compose --env-file=.env.bob --project-name=bob up radicle-node radicle-httpd
```

#### Run additional nodes in your network

For each additional node: 
1. Create a new profile in a new directory, using the steps in the "Create a profile" section above. 
1. Create a `.env.$nodename` file that will store all your environment variables:
```yaml
# IMPORTANT: substitute `<FIRST_NODE_ID>` with the node id of your **first** node ( `RAD_HOME=<path_to_first_node_profile> rad self | grep "Node ID"` ). 
# IMPORTANT: substitute `<FIRST_NODE_PROJECT_NAME>` with the `--project-name` value you used in your first node. In our example, this would be `bob`.
RADICLE_NODE_OPTIONS=--tracking-policy track --tracking-scope all  --connect <FIRST_NODE_ID>@<FIRST_NODE_PROJECT_NAME>_radicle-node_1.radicle-services:8776
# Use the new profile directory
RAD_ROOT=~/radicle/profiles/alice
# pick a new set of ports that are free on your machine
RADICLE_API_PORT=8887
RADICLE_NODE_PORT=8777
```
1. Start the containers:
```bash
# we don't need to start the included `caddy` service
docker-compose --env-file=.env.alice --project-name=alice up radicle-node radicle-httpd
```
1. The 2 nodes should now connect to each other ! You should be able to see a "Connected to <node id>" message and after a couple of minutes some Ping messages (and Pong responses) being exchanged.  

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md) and [HACKING.md](HACKING.md) for an
introduction to contributing to Radicle.

## License

Radicle is distributed under the terms of both the MIT license and the Apache License (Version 2.0).

See [LICENSE-APACHE](LICENSE-APACHE) and [LICENSE-MIT](LICENSE-MIT) for details.
