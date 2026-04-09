# Simulation Environment

A suite of tools to create simulated Radicle networks to run tests in.

This environment provisions a Kubernetes cluster, deploys a configurable topology of `radicle-node` instances, and provides a foundation for running cross-version, cross-platform, and adverse network tests.

## Prerequisites

To run the simulation environment, you need the following tools installed on your system:

- **[just](https://just.systems/)**: A command runner (replaces `make`).
- **[talosctl](https://talos.dev/docs/v1.12/learn-more/talosctl/)**: CLI for creating and managing Talos Linux clusters.
- **[kubectl](https://kubernetes.io/docs/tasks/tools/)**: CLI for interacting with Kubernetes.
- **[timoni](https://timoni.sh/)**: A package manager for Kubernetes, powered by CUE.
- **[cue](https://cuelang.org/)**: (Optional) Useful for debugging and formatting CUE files.
- **[QEMU](https://www.qemu.org/download/)** or **[Docker](https://www.docker.com/)**: Required by Talos to provision the local cluster nodes. (Defaults to `qemu`).

## Getting Started

The environment is managed entirely via `just`. From the `simulation` directory, you can run:

```shell
# Start the complete simulation (creates cluster, configures K8s, and deploys the network)
$ just start

# Note: To use Docker instead of QEMU, override the provisioner:
$ PROVISIONER=docker just start

# Inspect the cluster and see running pods
$ just show-cluster

# Tear down the network workloads (deletes pods and storage, keeps the cluster running)
$ just delete

# Destroy the entire Talos cluster and clean up your kubeconfig
$ just destroy
```

Run `just` by itself to see a list of all available commands.

## Architecture Overview

Here is how the different tools interact to build the simulation:

1. **Just (`justfile`)**: Acts as the orchestrator. It runs the bash scripts required to bootstrap the environment, verify tools are installed, and execute the correct CLI commands in sequence.
2. **Talos (`talosctl`)**: A lightweight, immutable Linux operating system built specifically to run Kubernetes. `just` uses `talosctl` to spin up a local K8s cluster inside QEMU or Docker.
3. **Kubernetes (`kubectl`)**: The orchestrator that runs the Radicle nodes in isolated pods. It manages their networking (DNS resolution between nodes) and persistent storage.
4. **Timoni & CUE**: The configuration engine. Instead of writing verbose YAML, we use CUE files to define network topologies. Timoni reads these CUE files, transpiles them into Kubernetes object definitions (StatefulSets, Services, ConfigMaps), and applies them to the cluster.

## Defining a Topology

Network topologies are defined in `instances/network.cue`. This file dictates how many nodes exist, what roles they play (e.g., `bootstrap`, `peer`), and how they connect to each other.

Here is an example of how the topology is structured:

```cue
package main

// ...

// Declare instances to deploy
values: {
	topology: {
		// A bootstrap node
		"bootstrap-v1-8-0": {
			role:          "bootstrap"
			version:       "1.8.0"
			replicas:      1
			nodeIdSeed:    "bootstrap-0" // Deterministically generates the NID above
			radicleConfig: #BaseBootstrapSeedConfig
		}
		
		// A peer node that connects to the bootstrap node
		"seed-v1-8-0": {
			role:          "seed"
			version:       "1.8.0"
			replicas:      1
			radicleConfig: #BasePeerConfig & {
				preferredSeeds: [
					// Uses a helper to format the K8s internal DNS address
					(#SeedAddress & {nid: #BootstrapNIDs["bootstrap-0"], name: "bootstrap-v1-8-0"}).out,
				]
			}
		}
	}
}
```

When you run `just start-network`, Timoni reads this file, merges it with the module definitions in `modules/radicle-node`, and deploys the resulting pods to Kubernetes.

## Helpful Commands

**Execute a command inside a node:**

```bash
$ kubectl exec peer-v1-8-0-0 -c node -- rad self

$ kubectl exec peer-v1-8-0-0 -it -c node -- sh
```

**Follow Radicle node events (from the sidecar):**

```bash
$ kubectl logs peer-v1-8-0-0 -c events -f
```

**View standard node logs:**

```bash
$ kubectl logs -f bootstrap-v1-8-0-0 -c node
```

**Describe a pod (Useful for debugging `CrashLoopBackOff` errors):**

```bash
$ kubectl describe pod bootstrap-v1-8-0-0
```

**Watch all cluster events in real-time:**

```bash
$ kubectl get events --watch
```

## Deterministic Keys

To ensure nodes can reliably connect to each other across restarts, the `bootstrap` nodes are configured with deterministic Node IDs (NIDs).

`bootstrap-0`: `did:key:z6MkhJ3cwzpAoNjFnJXWETSPHcDyw2HuBVEhgkyTfbjQHY1B`
`bootstrap-1`: `did:key:z6MkjcaeSHhQVJU1UeXpnHHZ6mp67zDfQYNMDotHGxbrk7Nj`
`bootstrap-2`: `did:key:z6MkjNGhuJvdp2noidRMLqco4jFnNNSWzCxSZH5nJV1pGrwQ`
`bootstrap-3`: `did:key:z6MkpEsXUMSnmyfwdEVkAKijTxGy9WKmNoHWpoxxLM6bbz9M`

## Why?

`heartwood` already has the following types of tests (as of 2026-04):

- [Unit](https://radicle.network/nodes/iris.radicle.network/rad:z3gqcJUoA1n9HaHKufZs5FCSGazv5/tree/crates/radicle/src/profile.rs#L842)
- [End-to-End](https://radicle.network/nodes/iris.radicle.network/rad:z3gqcJUoA1n9HaHKufZs5FCSGazv5/tree/crates/radicle-node/src/tests/e2e.rs)
- [CLI](https://radicle.network/nodes/iris.radicle.network/rad:z3gqcJUoA1n9HaHKufZs5FCSGazv5/tree/crates/radicle-cli/tests/commands/checkout.rs)

However we can only run them on the currently checked out version of `heartwood`, this leaves gaps in our testing coverage, particularly for cross-version and cross-platform testing.

The simulation environment is intended to remedy these gaps and more.
See the [Goals] section for more info.

## Constraints

### Non-Goals:

- Replace existing unit, CLI and end-to-end tests.
- Deterministic execution; Not creating an alternative [madsim](https://github.com/madsim-rs/madsim) or [shadow](https://shadow.github.io/).
- Writing custom orchestration code.
- Hard to reason about.

### Goals:

- [X] Isolation between simulations and main network.
- [X] Different node versions within a simulation.
- [ ] Cross platform ([Windows](https://github.com/dockur/windows), Linux & [macOS](https://github.com/dockur/macos)).
- [ ] Realistic load generation.
- [ ] Invariant assertion across simulation network.
- [ ] Teardown and Artifact collection.
- [ ] Continuous set of running simulations.
- [ ] Realtime Observability.
- [ ] CI/CD Integration.
- [ ] Cross simulation comparative insights e.g. CPU pressure change from version A to version B.
- [X] Flexibility to define network topologies.
- [ ] Easy to construct and run new simulations.
- [ ] Reproducible starting state.
- [ ] Adverse network emulation e.g. dropped packets, network delays...

## Plan

- [ ] Migrate existing [simulation environment repo](https://radicle.network/nodes/iris.radicle.network/rad%3Az2CzknCvAq9jSCpKdyjMppbvGmxyZ) into `heartwood`.
  1. [X] `radicle-node` timoni module.
  2. [ ] `radicle-node` custom container builder.
  3. [X] `instances` topology definition files.
  4. [ ] `sim-tests` rust crate.
  5. [X] `justfile` orchestration.
  6. [ ] `observability` definition files.
