# Simulation Environment

A suite of tools to create simulated Radicle networks to run tests in:

- **Talos**: A lightweight, immutable Linux operating system built specifically to run Kubernetes.
  It can run locally on your machine (via QEMU or Docker) or as a baremetal OS (amongst other deploy options).
- **Kubernetes (K8s)**: The orchestrator that runs the Radicle nodes in isolated pods and manages their networking and storage.
- **Timoni** & **CUE**: The configuration engine.
  Instead of writing YAML, we use CUE files to define network topologies.
  Timoni translates these into Kubernetes instructions.
- **Cargo test**: The test runner.
  Write tests in Rust that will execute over the provisioned networks.

## Why?

`heartwood` already has the following types of tests (as of 2026-04):

- [Unit](https://app.radicle.xyz/nodes/iris.radicle.xyz/rad:z3gqcJUoA1n9HaHKufZs5FCSGazv5/tree/crates/radicle/src/profile.rs#L842)
- [End-to-End](https://app.radicle.xyz/nodes/iris.radicle.xyz/rad:z3gqcJUoA1n9HaHKufZs5FCSGazv5/tree/crates/radicle-node/src/tests/e2e.rs)
- [CLI](https://app.radicle.xyz/nodes/iris.radicle.xyz/rad:z3gqcJUoA1n9HaHKufZs5FCSGazv5/tree/crates/radicle-cli/tests/commands/checkout.rs)

However we can only run them on the currently checked out version of `heartwood`, this leaves gaps in our testing coverage, particularly for cross-version and cross-platform testing.

The simulation environment is intended to remedy these gaps and more.
See the [Goals] section for more info.

## Overview

The Garden team currently deploys containerised versions of `radicle-node` into [Quay.io](https://quay.io/repository/radicle_garden/radicle-node?tab=tags&tag=latest).
We can utilise these containers inside of K8s configuration files to compose sets of pods.
These pods encapsulate `radicle-node` processes in different configurations, e.g. peer, seed or bootstrap.
Also, they might run different versions of `heartwood` (to facilitate cross-version testing),
and on different platforms (to facilitate cross-platform testing).
Each of these 'sets of pods' configuration will be considered a network topology, and defined in [CUE](https://cuelang.org/).
It allows us to write type safe configuration definitions instead of YAML.
We will then use [Timoni](https://timoni.sh/) to transpile these CUE defined network topologies into [K8s object definition files](https://kubernetes.io/docs/concepts/overview/working-with-objects/) and deploy them.
[Talos](https://talos.dev) will be used to run the K8s pods on; so we can easily switch between locally deployed, via QEMU or Docker, to baremetal on SBC's like Raspberry Pi's, or remotely in cloud environments.
Then with some glue and orchestration code we can utilise the `cargo test` runner to provision a network topology, run tests over it and tear it down again.
Finally we can insert observability systems into K8s so we can inspect and compare metrics and logs from different test runs.

This will give us the following workflow for constructing test scenarios:

1. Define a network topology of `radicle-node`'s on some platform(s) in CUE.
2. Write tests that interact with the `radicle-nodes` in Rust.
3. Run the tests.
4. Inspect / Debug via observability systems.

## Constraints

### Non-Goals:

- Replace existing unit, CLI and end-to-end tests.
- Deterministic execution; Not creating an alternative [madsim](https://github.com/madsim-rs/madsim) or [shadow](https://shadow.github.io/).
- Writing custom orchestration code.
- Hard to reason about.

### Goals:

- [ ] Isolation between simulations and main network.
- [ ] Different node versions within a simulation.
- [ ] Cross platform ([Windows](https://github.com/dockur/windows), Linux & [MacOS](https://github.com/dockur/macos)).
- [ ] Realistic load generation.
- [ ] Invariant assertion across simulation network.
- [ ] Teardown and Artifact collection.
- [ ] Continuous set of running simulations.
- [ ] Realtime Observability.
- [ ] CI/CD Integration.
- [ ] Cross simulation comparative insights e.g. CPU pressure change from version A to version B.
- [ ] Flexibility to define network topologies.
- [ ] Easy to construct and run new simulations.
- [ ] Reproducible starting state.
- [ ] Adverse network emulation e.g. dropped packets, network delays...

## Plan

- [ ] Migrate existing [simulation environment repo](https://app.radicle.xyz/nodes/iris.radicle.xyz/rad%3Az2CzknCvAq9jSCpKdyjMppbvGmxyZ) into `heartwood`.
  1. [ ] `radicle-node` timoni module.
  2. [ ] `radicle-node` custom container builder.
  3. [ ] `instances` topology definition files.
  4. [ ] `sim-tests` rust crate.
  5. [ ] `Makefile`.
  6. [ ] `observability` definition files.
