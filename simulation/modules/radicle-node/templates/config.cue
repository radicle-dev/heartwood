package templates

import (
	timoniv1 "timoni.sh/core/v1alpha1"
)

#NodeGroup: {
	role:       string
	replicas:   int | *1
	repository: string | *"quay.io/radicle_garden/radicle-node"
	pullPolicy: string | *"IfNotPresent"
	version:    string | *"latest"
	nodeIdSeed: string | *""

	storage: {
		className: string | *"local-path"
		size:      string | *"1Gi"
	}

	resources: {...}

	sidecars: {
		events: bool | *true
	}

	scripts: {
		init: string | *"""
			#!/bin/sh
			set -e

			KUBE_CONFIG_DIR=/tmp/config-source
			RAD_HOME=/home/radicle/.radicle
			RAD_CONFIG=${RAD_HOME}/config.json

			echo "[INIT] Hostname: $(hostname)"
			# --- STANDARD INIT LOGIC (User 11011) ---
			mkdir -p "${RAD_HOME}"

			if [ -f "${KUBE_CONFIG_DIR}/config.json" ]; then
			  cp "${KUBE_CONFIG_DIR}/config.json" "${RAD_CONFIG}"
			  echo "[INIT] Config copied successfully."
			else
			  echo "[INIT] ERROR: Source config not found."
			  exit 1
			fi
			"""
		start: string | *"""
			#!/bin/sh
			set -e

			RAD_HOME=/home/radicle/.radicle
			RAD_ALIAS=$(hostname)
			RAD_KEY=${RAD_HOME}/keys/radicle
			RAD_CONFIG=${RAD_HOME}/config.json

			# Configure the external address by prepending the pod's hostname.
			# We only do this for seeds and bootstraps to ensure proper routing.
			configure_external_address() {
			  # Extract the first external address, stripping JSON formatting
			  EXT_ADDRESS=$(rad config get node.externalAddresses | tr -d '[]" \\n' | cut -d',' -f1)
			  
			  if [ -n "$EXT_ADDRESS" ]; then
			    # Check if it already starts with the pod's hostname to prevent stuttering
			    case "$EXT_ADDRESS" in
			      ${RAD_ALIAS}.*)
			        echo "[START] External address already correct: ${EXT_ADDRESS}"
			        ;;
			      *)
			        rad config remove node.externalAddresses "${EXT_ADDRESS}"
			        NEW_ADDRESS="${RAD_ALIAS}.${EXT_ADDRESS}"
			        rad config push node.externalAddresses "${NEW_ADDRESS}"
			        echo "[START] Node's external address updated to: ${NEW_ADDRESS}"
			        ;;
			    esac
			  fi
			}

			#
			# Generate keys
			#
			if [ ! -f "${RAD_KEY}" ]; then
			  echo "[START] Generating identity for: ${RAD_ALIAS}..."
			  # We move the config out of the way so 'rad auth' doesn't complain
			  if [ -f "${RAD_CONFIG}" ]; then
			     mv "${RAD_CONFIG}" "${RAD_CONFIG}.bak"
			  fi

			  # RAD_KEYGEN_SEED requires a 32-byte hex string.
			  # We hash either the injected NODE_ID_SEED or the hostname to generate it.
			  if [ -n "${NODE_ID_SEED}" ]; then
			    export RAD_KEYGEN_SEED=$(echo "${NODE_ID_SEED}" | sha256sum | tr -d "\\n *-")
			  else
			    export RAD_KEYGEN_SEED=$(hostname | sha256sum | tr -d "\\n *-")
			  fi

			  rad auth --alias "${RAD_ALIAS}"

			  if [ -f "${RAD_CONFIG}.bak" ]; then
			     mv "${RAD_CONFIG}.bak" "${RAD_CONFIG}"
			  fi
			  echo "[START] Identity generated"
			fi

			#
			# Update config settings
			#
			echo "[START] Node's alias set to: $(rad config set node.alias "${RAD_ALIAS}")"

			if [ "\(role)" = "seed" ] || [ "\(role)" = "bootstrap" ]; then
			  configure_external_address
			fi

			#
			# Start node
			#
			echo "[START] Starting Radicle node..."
			exec rad node start --foreground
			"""
		events: string | *"""
			#!/bin/sh

			RAD_HOME=/home/radicle/.radicle
			RAD_NODE_SOCKET=${RAD_HOME}/node/control.sock

			echo "[EVENTS] Waiting for node socket..."
			while [ ! -S ${RAD_NODE_SOCKET} ]; do
			  sleep 1
			done
			echo "[EVENTS] Socket found. Streaming events..."
			exec rad node events
			"""
	}

	radicleConfig: {
		node: {
			// Automatically generate the base external address using the role.
			// The start.sh script will dynamically prepend the pod's hostname to this at boot.
			externalAddresses: [...string] | *["\(role).default.svc.cluster.local:8776"]
			// Set network to "test" by default.
			network: "test"
			...
		}
		...
	}
}

// Config defines the schema and defaults for the Instance values.
#Config: {
	kubeVersion!: string
	clusterVersion: timoniv1.#SemVer & {#Version: kubeVersion, #Minimum: "1.20.0"}
	moduleVersion!: string
	metadata: timoniv1.#Metadata & {#Version: moduleVersion}
	metadata: labels: timoniv1.#Labels
	metadata: annotations?: timoniv1.#Annotations

	// The topology map is merged with the #NodeGroup schema
	topology: [string]: #NodeGroup

	// Helper to generate metadata with a specific name
	#Meta: {
		name: string
		out: {
			"name": name
			namespace: metadata.namespace
			if metadata.annotations != _|_ {
				annotations: metadata.annotations
			}
		}
	}
}

// Instance takes the config values and outputs the Kubernetes objects.
#Instance: {
	config: #Config

	// Extract unique roles to create headless services
	let _roles = {
		for name, group in config.topology {
			"\(group.role)": true
		}
	}

	objects: {
		// Generate one Headless Service per role (e.g. seed, peer, bootstrap)
		for roleName, _ in _roles {
			"svc-\(roleName)": #Service & {
				#config: config
				#role:   roleName
			}
		}

		// Generate a StatefulSet and ConfigMap for each group in the topology
		for name, group in config.topology {
			"cm-\(name)": #ConfigMap & {
				#config: config
				#name:   name
				#group:  group
			}
			"sts-\(name)": #StatefulSet & {
				#config: config
				#name:   name
				#group:  group
				#cmName: name + "-config"
			}
		}
	}
}
