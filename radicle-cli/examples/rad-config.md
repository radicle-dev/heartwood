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
      "type": "dynamic"
    },
    "connect": [],
    "externalAddresses": [],
    "network": "main",
    "log": "INFO",
    "relay": "auto",
    "limits": {
      "routingMaxSize": 1000,
      "routingMaxAge": 604800,
      "gossipMaxAge": 1209600,
      "fetchConcurrency": 1,
      "maxOpenFiles": 4096,
      "rate": {
        "inbound": {
          "fillRate": 5.0,
          "capacity": 1024
        },
        "outbound": {
          "fillRate": 10.0,
          "capacity": 2048
        }
      },
      "connection": {
        "inbound": 128,
        "outbound": 16
      }
    },
    "workers": 8,
    "seedingPolicy": {
      "default": "block"
    }
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

You can set scalar values by path.

```
$ rad config set node.alias bob
bob
```

You can add a value to a collection by path.

```
$ rad config add web.pinned.repositories rad:z3TajuiHXifEDEX4qbJxe8nXr9ufi
rad:z3TajuiHXifEDEX4qbJxe8nXr9ufi
```

You can remove a value from a collection by path.

```
$ rad config remove web.pinned.repositories rad:z3TajuiHXifEDEX4qbJxe8nXr9ufi
$ rad config get web.pinned.repositories
```

Values that are not strictly required for a working configuration, such as optional values or additional user-defined values, can be deleted.

```
$ rad config set web.name alice
alice
$ rad config delete web.name
```

Values along the path will be created if necessary.

```
$ rad config set value.a.future.update.might.add.value 5
5
$ rad config add value.a.future.update.might.add.collection 1
1
```

```
$ rad config add node.array a
a
$ rad config add node.array b
a
b
```

Values that are required for a valid config can't be deleted.

```fail
$ rad config delete node.alias
✗ Error: configuration JSON error: missing field `alias`
```

Values for changes are being validated.

```fail
$ rad config set web.pinned.repositories 5
✗ Error: configuration JSON error: invalid type: integer `5`, expected a sequence
```

The type of the operation is validated.

```fail
$ rad config add node.alias eve
✗ Error: the element at the path 'node.alias' is not a JSON array
```

