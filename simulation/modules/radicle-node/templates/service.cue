package templates

import (
	corev1 "k8s.io/api/core/v1"
)

#Service: corev1.#Service & {
	#config: #Config
	#role:   string
	apiVersion: "v1"
	kind:       "Service"
	metadata:   (#config.#Meta & {name: #role}).out
	spec: corev1.#ServiceSpec & {
		clusterIP: "None" // Headless service for direct pod DNS resolution
		selector: {
			"app":  "radicle-node"
			"role": #role
		}
		ports: [
			{
				name:       "gossip"
				port:       8776
				targetPort: 8776
			},
		]
	}
}
