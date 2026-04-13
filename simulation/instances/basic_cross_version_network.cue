@if(!debug)

package main

values: {
	topology: {
		"bootstrap-v1-6-1": {
			role:          "bootstrap"
			version:       "1.6.1"
			replicas:      1
			nodeIdSeed:    "bootstrap-0"
			radicleConfig: #BaseBootstrapSeedConfig
		}

		"bootstrap-v1-7-0": {
			role:          "bootstrap"
			version:       "1.7.0"
			replicas:      1
			nodeIdSeed:    "bootstrap-1"
			radicleConfig: #BaseBootstrapSeedConfig
		}

		"peer-v1-5-0": {
			role:          "peer"
			version:       "1.5.0"
			replicas:      1
			radicleConfig: #BasePeerConfig & {
				preferredSeeds: [
					(#SeedAddress & {nid: #BootstrapNIDs["bootstrap-0"], name: "bootstrap-v1-6-1"}).out,
					(#SeedAddress & {nid: #BootstrapNIDs["bootstrap-1"], name: "bootstrap-v1-7-0"}).out,
				]
			}
		}
		"peer-v1-6-0": {
			role:          "peer"
			version:       "1.6.0"
			replicas:      1
			radicleConfig: #BasePeerConfig & {
				preferredSeeds: [
					(#SeedAddress & {nid: #BootstrapNIDs["bootstrap-0"], name: "bootstrap-v1-6-1"}).out,
					(#SeedAddress & {nid: #BootstrapNIDs["bootstrap-1"], name: "bootstrap-v1-7-0"}).out,
				]
			}
		}
		"peer-v1-6-1": {
			role:          "peer"
			version:       "1.6.1"
			replicas:      1
			radicleConfig: #BasePeerConfig & {
				preferredSeeds: [
					(#SeedAddress & {nid: #BootstrapNIDs["bootstrap-0"], name: "bootstrap-v1-6-1"}).out,
					(#SeedAddress & {nid: #BootstrapNIDs["bootstrap-1"], name: "bootstrap-v1-7-0"}).out,
				]
			}
		}
		"peer-v1-7-0": {
			role:          "peer"
			version:       "1.7.0"
			replicas:      2
			radicleConfig: #BasePeerConfig & {
				preferredSeeds: [
					(#SeedAddress & {nid: #BootstrapNIDs["bootstrap-0"], name: "bootstrap-v1-6-1"}).out,
					(#SeedAddress & {nid: #BootstrapNIDs["bootstrap-1"], name: "bootstrap-v1-7-0"}).out,
				]
			}
		}
		"peer-v1-7-1": {
			role:          "peer"
			version:       "1.7.1"
			replicas:      2
			radicleConfig: #BasePeerConfig & {
				preferredSeeds: [
					(#SeedAddress & {nid: #BootstrapNIDs["bootstrap-0"], name: "bootstrap-v1-6-1"}).out,
					(#SeedAddress & {nid: #BootstrapNIDs["bootstrap-1"], name: "bootstrap-v1-7-0"}).out,
				]
			}
		}
		"peer-v1-8-0": {
			role:          "peer"
			version:       "1.8.0"
			replicas:      1
			radicleConfig: #BasePeerConfig & {
				preferredSeeds: [
					(#SeedAddress & {nid: #BootstrapNIDs["bootstrap-0"], name: "bootstrap-v1-6-1"}).out,
					(#SeedAddress & {nid: #BootstrapNIDs["bootstrap-1"], name: "bootstrap-v1-7-0"}).out,
				]
			}
		}
	}
}
