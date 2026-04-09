package templates

import (
	"encoding/json"
	corev1 "k8s.io/api/core/v1"
)

#ConfigMap: corev1.#ConfigMap & {
	#config: #Config
	#name:   string
	#group:  #NodeGroup
	apiVersion: "v1"
	kind:       "ConfigMap"
	metadata:   (#config.#Meta & {name: #name + "-config"}).out
	data: {
		"config.json": json.Marshal(#group.radicleConfig)
	}
}
