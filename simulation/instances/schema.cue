@if(!debug)

package main

//
// Pre-calculated NIDs.
//
#BootstrapNIDs: {
	"bootstrap-0": "z6MkhJ3cwzpAoNjFnJXWETSPHcDyw2HuBVEhgkyTfbjQHY1B"
	"bootstrap-1": "z6MkjcaeSHhQVJU1UeXpnHHZ6mp67zDfQYNMDotHGxbrk7Nj"
	"bootstrap-2": "z6MkjNGhuJvdp2noidRMLqco4jFnNNSWzCxSZH5nJV1pGrwQ"
	"bootstrap-3": "z6MkpEsXUMSnmyfwdEVkAKijTxGy9WKmNoHWpoxxLM6bbz9M"
}

//
// Shared configs
//
#SeedAddress: {
	nid:   string
	name:  string
	role:  string | *"bootstrap"
	index: int | *0
	out:   "\(nid)@\(name)-\(index).\(role).default.svc.cluster.local:8776"
}

#BaseBootstrapSeedConfig: {
	node: {
		listen: ["0.0.0.0:8776"]
		seedingPolicy: {
			default: "allow"
			scope:   "all"
		}
		...
	}
	...
}

#BasePeerConfig: {
	node: {
		listen: []
		peers: type: "dynamic"
		connect: []
		externalAddresses: [] // Explicitly override the default to be empty
		log:                  "INFO"
		relay:                "auto"
		limits: {
			routingMaxSize:   1000
			routingMaxAge:    604800
			gossipMaxAge:     1209600
			fetchConcurrency: 1
			maxOpenFiles:     4096
			rate: {
				inbound: {fillRate: 5.0, capacity: 1024}
				outbound: {fillRate: 10.0, capacity: 2048}
			}
			connection: {inbound: 128, outbound: 16}
			fetchPackReceive: "500.0 MiB"
		}
		seedingPolicy: default: "block"
		...
	}
	...
}

//
// Topology Constraints
//
values: {
	topology: {
		//
		// Naming conventions so bindings can be generated automatically
		// See: [`radicle-simulations/build.rs`]
		//

		// Prefix enforcement `bootstrap-`, `peer-`, `seed-`
		[=~"^(bootstrap|peer|seed)-[a-zA-Z0-9-]+$"]: {
			role:     string
			replicas: int | *1
		}
		// Peer version enforcement
		[=~"^peer-v[0-9]+-[0-9]+-[0-9]+$"]: {
			role: "peer"
		}
	}
}
