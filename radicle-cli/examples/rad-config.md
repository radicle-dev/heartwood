The `rad config` command is used to manage the local user configuration.
In its simplest form, `rad config` prints the current configuration.

```
$ rad config
{
  "publicExplorer": "https://app.radicle.xyz/nodes/$host/$rid$path",
  "preferredSeeds": [
    "z6MkrLMMsiPWUcNPHcRajuMi9mDfYckSoJyPwwnknocNYPm7@seed.radicle.garden:8776",
    "z6MksmpU5b1dS7oaqF2bHXhQi1DWy2hB7Mh9CuN7y1DN6QSz@seed.radicle.xyz:8776"
  ],
  "web": {
    "pinned": {
      "repositories": []
    }
  },
  "cli": {
    "hints": true
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
    "network": "main",
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
    "scope": "all"
  }
}
```

You can also get any value in the configuration by path, eg.

```
$ rad config get node.alias
alice
$ rad config get preferredSeeds
z6MkrLMMsiPWUcNPHcRajuMi9mDfYckSoJyPwwnknocNYPm7@seed.radicle.garden:8776
z6MksmpU5b1dS7oaqF2bHXhQi1DWy2hB7Mh9CuN7y1DN6QSz@seed.radicle.xyz:8776
$ rad config get node.limits.routingMaxSize
1000
```
