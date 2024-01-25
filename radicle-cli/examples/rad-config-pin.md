The configuration file keeps track of repositories that we want to
pin, which can be used in web applications and their landing pages.
To do this we can use `rad config pin` in the context of a repository:

```
$ rad config pin rad:z42hL2jL4XNk6K8oHQaSWfMgCL7ji
✓ Successfully pinned rad:z42hL2jL4XNk6K8oHQaSWfMgCL7ji
```

We can see that it was added:

```
$ rad config
{
  "publicExplorer": "https://app.radicle.xyz/nodes/$host/$rid$path",
  "preferredSeeds": [],
  "web": {
    "pinned": {
      "repositories": [
        "rad:z42hL2jL4XNk6K8oHQaSWfMgCL7ji"
      ]
    }
  },
  "cli": {
    "hints": false
  },
  "node": {
    "alias": "alice",
    "listen": [],
    "peers": {
      "type": "dynamic",
      "target": 8
    },
    "connect": [],
    "externalAddresses": [],
    "network": "test",
    "relay": true,
    "limits": {
      "routingMaxSize": 1000,
      "routingMaxAge": 604800,
      "gossipMaxAge": 1209600,
      "fetchConcurrency": 1,
      "maxOpenFiles": 4096,
      "rate": {
        "inbound": {
          "fillRate": 0.2,
          "capacity": 32
        },
        "outbound": {
          "fillRate": 1.0,
          "capacity": 64
        }
      }
    },
    "policy": "block",
    "scope": "followed"
  }
}
```

We can also add an RID that is not associated with the current
repository if we specify it:

```
$ rad config pin rad:z3gqcJUoA1n9HaHKufZs5FCSGazv5
✓ Successfully pinned rad:z3gqcJUoA1n9HaHKufZs5FCSGazv5
```

To remove that RID we can simply `unpin` it:

```
$ rad config unpin rad:z3gqcJUoA1n9HaHKufZs5FCSGazv5
✓ Successfully unpinned rad:z3gqcJUoA1n9HaHKufZs5FCSGazv5
```
