# radicle-node

A [timoni.sh](http://timoni.sh) module for deploying radicle-node to Kubernetes clusters.

## Prerequisites

* [Timoni](https://timoni.sh/) installed.
* A valid `KUBECONFIG` pointing to your Kubernetes cluster.
* A default `StorageClass` available in your cluster (used by the StatefulSet for persistent storage).

## Install

To create an instance using the default values:

```shell
timoni -n default apply radicle-node oci://<container-registry-url>
```

To change the [default configuration](#configuration),
create one or more `values.cue` files and apply them to the instance.

For example, create a file `my-values.cue` with the following content:

```cue
values: {
	topology: {
		"my-seed-node": {
			role: "seed"
			replicas: 1
			storage: size: "5Gi"
			sidecars: events: true
		}
	}
}
```

And apply the values with:

```shell
timoni -n default apply radicle-node oci://<container-registry-url> \
--values ./my-values.cue
```

## Uninstall

To uninstall an instance and delete all its Kubernetes resources:

```shell
timoni -n default delete radicle-node
```

## Module Structure

This Timoni module is organized into several CUE files that define the schema, templates, and deployment logic:

* **`timoni.cue`**: The entry point of the module. It defines the user-supplied values schema and the Timoni workflow (how to build, validate, and apply the Kubernetes resources).
* **`templates/config.cue`**: Contains the core `#Config` and `#NodeGroup` schemas. It defines default values (including startup scripts) and the `#Instance` logic that iterates over the defined `topology` to generate the required Kubernetes objects.
* **`templates/statefulset.cue`**: Defines the Kubernetes `StatefulSet` template, configuring the Radicle node container, init containers for configuration prep, optional sidecars (like the events logger), and persistent volume claims.
* **`templates/configmap.cue`**: Defines the Kubernetes `ConfigMap` template used to inject the `config.json` into the Radicle node pods.
* **`templates/service.cue`**: Defines a Kubernetes Headless `Service` template. This is created per "role" to allow direct pod-to-pod DNS resolution for Radicle's gossip network.
* **`debug_tool.cue`**: Provides CUE CLI commands (`build` and `ls`) to render and inspect the generated Kubernetes manifests locally without applying them to a cluster.
* **`timoni.ignore`**: Lists files and directories that should be excluded when packaging or applying the module.

## Configuration

### Topology Configuration

This module uses a `topology` map to define one or more groups of Radicle nodes. Each group generates its own `StatefulSet` and `ConfigMap`, and groups sharing the same `role` will share a headless `Service`.

| Key                               | Type     | Default                                 | Description                                                                 |
|-----------------------------------|----------|-----------------------------------------|-----------------------------------------------------------------------------|
| `topology: [name]: role`          | `string` | **Required**                            | The role of the node group (e.g. `seed`, `peer`). Used for DNS resolution. |
| `topology: [name]: replicas`      | `int`    | `1`                                     | Number of pod replicas for this group.                                      |
| `topology: [name]: repository`    | `string` | `quay.io/radicle_garden/radicle-node`   | Container image repository.                                                 |
| `topology: [name]: version`       | `string` | `latest`                                | Container image tag.                                                        |
| `topology: [name]: storage`       | `object` | `{className: "local-path", size: "1Gi"}`| Persistent Volume Claim configuration for `/home/radicle/.radicle`.         |
| `topology: [name]: sidecars`      | `object` | `{events: true}`                        | Toggles sidecar containers (e.g. the events logger).                       |
| `topology: [name]: radicleConfig` | `object` | `{node: {network: "test", …}}`        | The contents of the `config.json` injected into the node.                   |

## Node Identity / Secrets

The node's identity (cryptographic keys) is generated automatically on startup if it doesn't exist. You can control this generation deterministically using the `NODE_ID_SEED` environment variable.

If `NODE_ID_SEED` is provided in the configuration, the startup script hashes it to generate a 32-byte seed for `rad auth`. If omitted, the pod's hostname is hashed instead. This ensures that if a pod restarts, it can regenerate the same identity, or you can explicitly pass a seed to persist identities across complete cluster teardowns.

## Debugging

You can render and inspect the generated Kubernetes manifests locally without applying them to a cluster using the included CUE debug tool:

```shell
# Print a summary of the resources that will be created
cue cmd -t debug -t name=my-node -t namespace=default -t mv=1.0.0 -t kv=1.28.0 ls

# Output the full multi-doc YAML
cue cmd -t debug -t name=my-node -t namespace=default -t mv=1.0.0 -t kv=1.28.0 build
```
