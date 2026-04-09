package templates

import (
	appsv1 "k8s.io/api/apps/v1"
	corev1 "k8s.io/api/core/v1"
)

#StatefulSet: appsv1.#StatefulSet & {
	#config: #Config
	#name:   string
	#group:  #NodeGroup
	#cmName: string
	apiVersion: "apps/v1"
	kind:       "StatefulSet"
	metadata:   (#config.#Meta & {name: #name}).out
	spec: appsv1.#StatefulSetSpec & {
		serviceName: #group.role
		replicas:    #group.replicas
		selector: matchLabels: {
			"app":      "radicle-node"
			"instance": #name
		}
		template: {
			metadata: labels: {
				"app":      "radicle-node"
				"role":     #group.role
				"instance": #name
			}
			spec: corev1.#PodSpec & {
				securityContext: {
					fsGroup: 11011
					seccompProfile: type: "RuntimeDefault"
					runAsNonRoot: true
					runAsUser:    11011
					runAsGroup:   11011
				}
				initContainers: [
					{
						name:  "config-prep"
						image: "busybox"
						command: ["sh", "-c"]
						args: [#group.scripts.init]
						volumeMounts: [
							{
								name:      "config-template"
								mountPath: "/tmp/config-source"
							},
							{
								name:      "radicle-home"
								mountPath: "/home/radicle/.radicle"
							},
						]
						securityContext: {
							runAsUser:                11011
							runAsNonRoot:             true
							allowPrivilegeEscalation: false
							capabilities: drop: ["ALL"]
							seccompProfile: type: "RuntimeDefault"
						}
					},
				]
				containers: [
					{
						name:            "node"
						image:           "\(#group.repository):\(#group.version)"
						imagePullPolicy: #group.pullPolicy
						command: ["/bin/sh", "-c"]
						args: [#group.scripts.start]
						env: [
							{
								name:  "RAD_PASSPHRASE"
								value: ""
							},
							{
								name:  "NODE_ID_SEED"
								value: #group.nodeIdSeed
							},
						]
						securityContext: {
							allowPrivilegeEscalation: false
							capabilities: drop: ["ALL"]
							privileged:             false
							readOnlyRootFilesystem: false
						}
						ports: [
							{
								containerPort: 8776
								name:          "gossip"
							},
						]
						volumeMounts: [
							{
								name:      "radicle-home"
								mountPath: "/home/radicle/.radicle"
							},
						]
					},
					if #group.sidecars.events {
						{
							name:  "events"
							image: "\(#group.repository):\(#group.version)"
							command: ["/bin/sh", "-c"]
							args: [#group.scripts.events]
							securityContext: {
								runAsNonRoot:             true
								runAsUser:                11011
								runAsGroup:               11011
								allowPrivilegeEscalation: false
								capabilities: drop: ["ALL"]
								readOnlyRootFilesystem: false
							}
							volumeMounts: [
								{
									name:      "radicle-home"
									mountPath: "/home/radicle/.radicle"
								},
							]
						}
					},
				]
				volumes: [
					{
						name: "config-template"
						configMap: name: #cmName
					},
				]
			}
		}
		volumeClaimTemplates: [
			{
				metadata: {
					name: "radicle-home"
					labels: {
						"app":      "radicle-node"
						"role":     #group.role
						"instance": #name
					}
				}
				spec: {
					storageClassName: #group.storage.className
					accessModes: ["ReadWriteOnce"]
					resources: requests: storage: #group.storage.size
				}
			},
		]
	}
}
