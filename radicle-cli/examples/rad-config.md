The `rad config` command is used to manage the local user configuration.
In its simplest form, `rad config` prints the current configuration.

```
$ rad config
{
  "publicExplorer": "https://app.radicle.xyz/nodes/$host/$rid$path",
  "preferredSeeds": [],
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
