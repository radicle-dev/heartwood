The `rad config` command is used to manage the local user configuration.
In its simplest form, `rad config` prints the current configuration.

```
$ rad config
{
  "publicExplorer": "https://radicle.network/nodes/$host/$rid$path",
  "preferredSeeds": [
    "z6MkrLMMsiPWUcNPHcRajuMi9mDfYckSoJyPwwnknocNYPm7@iris.radicle.network:8776"
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
      },
      "fetchPackReceive": "500.0 MiB"
    },
    "workers": 8,
    "seedingPolicy": {
      "default": "block"
    }
  }
}
```

The `rad config schema` command provides the JSON schema that can be used to
validate the JSON of the user configuration.

```
$ rad config schema
{
  "$schema": "https://json-schema.org/draft/2020-12/schema",
  "title": "Config",
  "description": "Local Radicle configuration.",
  "type": "object",
  "properties": {
    "publicExplorer": {
      "description": "Public explorer. This is used for generating links.",
      "$ref": "#/$defs/Explorer",
      "default": "https://radicle.network/nodes/$host/$rid$path"
    },
    "preferredSeeds": {
      "description": "Preferred seeds. These seeds will be used for explorer links/nand in other situations when a seed needs to be chosen.",
      "type": "array",
      "items": {
        "$ref": "#/$defs/ConnectAddress"
      },
      "default": []
    },
    "web": {
      "description": "Web configuration.",
      "$ref": "#/$defs/WebConfig",
      "default": {
        "pinned": {
          "repositories": []
        }
      }
    },
    "cli": {
      "description": "CLI configuration.",
      "$ref": "#/$defs/CliConfig",
      "default": {
        "hints": true
      }
    },
    "node": {
      "description": "Node configuration.",
      "$ref": "#/$defs/NodeConfig"
    }
  },
  "required": [
    "node"
  ],
  "$defs": {
    "Explorer": {
      "description": "A public explorer.",
      "type": "string"
    },
    "ConnectAddress": {
      "description": "A node address to connect to. Format: An Ed25519 public key in multibase encoding, followed by the symbol '@', followed by an IP address, or a DNS name, or a Tor onion name, or an I2P address, followed by the symbol ':', followed by a TCP port number.",
      "type": "string",
      "pattern": "^.+@.+:((6553[0-5])|(655[0-2][0-9])|(65[0-4][0-9]{2})|(6[0-4][0-9]{3})|([1-5][0-9]{4})|([0-5]{0,5})|([0-9]{1,4}))$",
      "examples": [
        "z6MkrLMMsiPWUcNPHcRajuMi9mDfYckSoJyPwwnknocNYPm7@rosa.radicle.network:8776",
        "z6MkvUJtYD9dHDJfpevWRT98mzDDpdAtmUjwyDSkyqksUr7C@xmrhfasfg5suueegrnc4gsgyi2tyclcy5oz7f5drnrodmdtob6t2ioyd.onion:8776",
        "z6Mkvky2mnSYCTUMKRdAUoZXBXLLKtnWEkWeYQcGjjnmobAU@f2atcc7udeub5kh4nkljtjwyk7ikjviorufzgwnfwhkphljl3vhq.b32.i2p:8776",
        "z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi@seed.example.com:8776",
        "z6MkkfM3tPXNPrPevKr3uSiQtHPuwnNhu2yUVjgd2jXVsVz5@192.0.2.0:31337"
      ]
    },
    "WebConfig": {
      "description": "Web configuration.",
      "type": "object",
      "properties": {
        "pinned": {
          "description": "Pinned content.",
          "$ref": "#/$defs/Pinned"
        },
        "bannerUrl": {
          "description": "URL pointing to an image used in the header of a node page.",
          "type": [
            "string",
            "null"
          ],
          "format": "uri"
        },
        "avatarUrl": {
          "description": "URL pointing to an image used as the node avatar.",
          "type": [
            "string",
            "null"
          ],
          "format": "uri"
        },
        "description": {
          "description": "Node description.",
          "type": [
            "string",
            "null"
          ],
          "format": "uri"
        }
      },
      "required": [
        "pinned"
      ]
    },
    "Pinned": {
      "description": "Pinned content. This can be used to pin certain content when/nlisting, e.g. pin repositories on a web client.",
      "type": "object",
      "properties": {
        "repositories": {
          "description": "Pinned repositories.",
          "type": "array",
          "uniqueItems": true,
          "items": {
            "$ref": "#/$defs/RepoId"
          }
        }
      },
      "required": [
        "repositories"
      ]
    },
    "RepoId": {
      "description": "A repository identifier.",
      "type": "string",
      "examples": [
        "rad:z3gqcJUoA1n9HaHKufZs5FCSGazv5"
      ],
      "minLength": 5,
      "pattern": "rad:z[1-9a-km-zA-HJ-NP-Z]+"
    },
    "CliConfig": {
      "description": "CLI configuration.",
      "type": "object",
      "properties": {
        "hints": {
          "description": "Whether to show hints or not in the CLI.",
          "type": "boolean",
          "default": false
        }
      }
    },
    "NodeConfig": {
      "description": "Service configuration.",
      "type": "object",
      "properties": {
        "alias": {
          "description": "Node alias.",
          "$ref": "#/$defs/Alias"
        },
        "userAgent": {
          "description": "User agent string to advertise in the node announcement, which is sent out to other nodes.",
          "anyOf": [
            {
              "$ref": "#/$defs/UserAgent"
            },
            {
              "type": "null"
            }
          ]
        },
        "listen": {
          "description": "Socket address (a combination of IPv4 or IPv6 address and TCP port) to listen on.",
          "type": "array",
          "items": {
            "type": "string"
          },
          "examples": [
            "127.0.0.1:8776"
          ],
          "default": []
        },
        "peers": {
          "description": "Peer configuration.",
          "$ref": "#/$defs/PeerConfig",
          "default": {
            "type": "dynamic"
          }
        },
        "connect": {
          "description": "Peers to connect to on startup./nConnections to these peers will be maintained.",
          "type": "array",
          "uniqueItems": true,
          "items": {
            "$ref": "#/$defs/ConnectAddress"
          },
          "default": []
        },
        "externalAddresses": {
          "description": "Specify the node's public addresses",
          "type": "array",
          "items": {
            "$ref": "#/$defs/Address"
          },
          "default": []
        },
        "proxy": {
          "description": "Global proxy.",
          "type": [
            "string",
            "null"
          ]
        },
        "onion": {
          "description": "Onion address config.",
          "$ref": "#/$defs/AddressConfig"
        },
        "i2p": {
          "description": "I2P address config.",
          "$ref": "#/$defs/AddressConfig"
        },
        "network": {
          "description": "Peer-to-peer network.",
          "$ref": "#/$defs/Network",
          "default": "main"
        },
        "log": {
          "description": "Log level.",
          "$ref": "#/$defs/LogLevel",
          "default": "INFO"
        },
        "relay": {
          "description": "Whether or not our node should relay messages.",
          "$ref": "#/$defs/Relay",
          "default": "auto"
        },
        "limits": {
          "description": "Configured service limits.",
          "$ref": "#/$defs/Limits",
          "default": {
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
            },
            "fetchPackReceive": "500.0 MiB"
          }
        },
        "workers": {
          "description": "Number of worker threads to spawn.",
          "type": "integer",
          "format": "uint",
          "minimum": 0,
          "default": 8
        },
        "seedingPolicy": {
          "description": "Default seeding policy.",
          "$ref": "#/$defs/DefaultSeedingPolicy",
          "default": {
            "default": "block"
          }
        },
        "database": {
          "description": "Database configuration.",
          "$ref": "#/$defs/Config"
        },
        "fetch": {
          "description": "Configuration for fetching from other nodes.",
          "$ref": "#/$defs/Fetch"
        },
        "secret": {
          "description": "Path to a file containing an Ed25519 secret key, in OpenSSH format, i.e./nwith the `-----BEGIN OPENSSH PRIVATE KEY-----` header. The corresponding/npublic key will be used as the Node ID./n/nA decryption password cannot be configured, but passed at runtime via/nthe environment variable `RAD_PASSPHRASE`.",
          "type": [
            "string",
            "null"
          ]
        }
      },
      "required": [
        "alias"
      ],
      "additionalProperties": true
    },
    "Alias": {
      "description": "Node alias, i.e. a short and memorable name for it.",
      "type": "string"
    },
    "UserAgent": {
      "description": "A user agent string that starts and ends with the symbol '/', and contains segments of the form 'client:version' separated by '/'. The client and version parts must be non-empty, and must consist of printable ASCII characters excluding '/' and ':'. The entire string must be at most 64 characters long.",
      "type": "string",
      "minLength": 3,
      "maxLength": 64,
      "examples": [
        "/radicle:1.9.0/",
        "/example:42.0.0/other-client:2.3.4/"
      ],
      "pattern": "^/([^:///s]+((:[^:///s]+))?/)+$"
    },
    "PeerConfig": {
      "description": "Peer configuration.",
      "oneOf": [
        {
          "description": "Static peer set. Connect to the configured peers and maintain the connections.",
          "type": "object",
          "properties": {
            "type": {
              "type": "string",
              "const": "static"
            }
          },
          "required": [
            "type"
          ]
        },
        {
          "description": "Dynamic peer set.",
          "type": "object",
          "properties": {
            "type": {
              "type": "string",
              "const": "dynamic"
            }
          },
          "required": [
            "type"
          ]
        }
      ]
    },
    "Address": {
      "description": "An IP address, or a DNS name, or a Tor onion name, or an I2P address,followed by the symbol ':', followed by a TCP port number.",
      "type": "string",
      "examples": [
        "xmrhfasfg5suueegrnc4gsgyi2tyclcy5oz7f5drnrodmdtob6t2ioyd.onion:8776",
        "f2atcc7udeub5kh4nkljtjwyk7ikjviorufzgwnfwhkphljl3vhq.b32.i2p:8776",
        "seed.example.com:8776",
        "192.0.2.0:31337"
      ],
      "pattern": "^.+:((6553[0-5])|(655[0-2][0-9])|(65[0-4][0-9]{2})|(6[0-4][0-9]{3})|([1-5][0-9]{4})|([0-5]{0,5})|([0-9]{1,4}))$"
    },
    "AddressConfig": {
      "description": "Proxy configuration.",
      "oneOf": [
        {
          "description": "Proxy connections to this address type.",
          "type": "object",
          "properties": {
            "address": {
              "description": "Proxy address.",
              "type": "string"
            },
            "mode": {
              "type": "string",
              "const": "proxy"
            }
          },
          "required": [
            "mode",
            "address"
          ]
        },
        {
          "description": "Forward address to the next layer. Either this is the global proxy,/nor the operating system, via DNS.",
          "type": "object",
          "properties": {
            "mode": {
              "type": "string",
              "const": "forward"
            }
          },
          "required": [
            "mode"
          ]
        },
        {
          "description": "Drop connections to this address type.",
          "type": "object",
          "properties": {
            "mode": {
              "type": "string",
              "const": "drop"
            }
          },
          "required": [
            "mode"
          ]
        }
      ]
    },
    "Network": {
      "description": "Peer-to-peer network.",
      "type": "string",
      "enum": [
        "main",
        "test"
      ]
    },
    "LogLevel": {
      "$ref": "#/$defs/Level"
    },
    "Level": {
      "description": "A log level.",
      "oneOf": [
        {
          "description": "Designates very serious errors.",
          "type": "string",
          "const": "ERROR"
        },
        {
          "description": "Designates hazardous situations.",
          "type": "string",
          "const": "WARN"
        },
        {
          "description": "Designates useful information.",
          "type": "string",
          "const": "INFO"
        },
        {
          "description": "Designates lower priority information.",
          "type": "string",
          "const": "DEBUG"
        },
        {
          "description": "Designates very low priority, often extremely verbose, information.",
          "type": "string",
          "const": "TRACE"
        }
      ]
    },
    "Relay": {
      "description": "Relay configuration.",
      "oneOf": [
        {
          "description": "Always relay messages.",
          "type": "string",
          "const": "always"
        },
        {
          "description": "Never relay messages.",
          "type": "string",
          "const": "never"
        },
        {
          "description": "Relay messages when applicable.",
          "type": "string",
          "const": "auto"
        }
      ]
    },
    "Limits": {
      "description": "Configuration parameters defining attributes of minima and maxima.",
      "type": "object",
      "properties": {
        "routingMaxSize": {
          "description": "Number of routing table entries before we start pruning.",
          "type": "integer",
          "format": "uint",
          "minimum": 0,
          "default": 1000
        },
        "routingMaxAge": {
          "description": "How long to keep a routing table entry before being pruned.",
          "$ref": "#/$defs/LocalDuration",
          "default": 604800
        },
        "gossipMaxAge": {
          "description": "How long to keep a gossip message entry before pruning it.",
          "$ref": "#/$defs/LocalDuration",
          "default": 1209600
        },
        "fetchConcurrency": {
          "description": "Maximum number of concurrent fetches per peer connection.",
          "type": "integer",
          "format": "uint",
          "minimum": 0,
          "default": 1
        },
        "maxOpenFiles": {
          "description": "Maximum number of open files.",
          "type": "integer",
          "format": "uint",
          "minimum": 0,
          "default": 4096
        },
        "rate": {
          "description": "Rate limiter settings.",
          "$ref": "#/$defs/RateLimits",
          "default": {
            "inbound": {
              "fillRate": 5.0,
              "capacity": 1024
            },
            "outbound": {
              "fillRate": 10.0,
              "capacity": 2048
            }
          }
        },
        "connection": {
          "description": "Connection limits.",
          "$ref": "#/$defs/ConnectionLimits",
          "default": {
            "inbound": 128,
            "outbound": 16
          }
        },
        "fetchPackReceive": {
          "description": "Channel limits.",
          "$ref": "#/$defs/FetchPackSizeLimit",
          "default": "500.0 MiB"
        }
      }
    },
    "LocalDuration": {
      "description": "A time duration measured locally in seconds.",
      "type": "integer",
      "format": "uint128",
      "minimum": 0
    },
    "RateLimits": {
      "description": "Rate limits for inbound and outbound connections.",
      "type": "object",
      "properties": {
        "inbound": {
          "$ref": "#/$defs/RateLimit"
        },
        "outbound": {
          "$ref": "#/$defs/RateLimit"
        }
      },
      "required": [
        "inbound",
        "outbound"
      ]
    },
    "RateLimit": {
      "description": "Rate limits for a single connection.",
      "type": "object",
      "properties": {
        "fillRate": {
          "type": "number",
          "format": "double"
        },
        "capacity": {
          "type": "integer",
          "format": "uint",
          "minimum": 0
        }
      },
      "required": [
        "fillRate",
        "capacity"
      ]
    },
    "ConnectionLimits": {
      "description": "Connection limits.",
      "type": "object",
      "properties": {
        "inbound": {
          "description": "Max inbound connections.",
          "type": "integer",
          "format": "uint",
          "minimum": 0,
          "default": 128
        },
        "outbound": {
          "description": "Max outbound connections. Note that this can be greater than the *target* number.",
          "type": "integer",
          "format": "uint",
          "minimum": 0,
          "default": 16
        }
      }
    },
    "FetchPackSizeLimit": {
      "description": "Limiter for byte streams./n/nDefault: 500MiB",
      "$ref": "#/$defs/ByteSize"
    },
    "ByteSize": {
      "description": "Byte quantities using unit prefixes according to SI or ISO/IEC 80000-13.",
      "type": "string",
      "pattern": "^//d+(//.//d+)? ((K|M|G|T|P)i?B?|B)$",
      "examples": [
        "7 G",
        "50.3 TiB",
        "200 B",
        "4 Ki",
        "10 MB"
      ]
    },
    "DefaultSeedingPolicy": {
      "description": "Default seeding policy. Applies when no repository policies for the given repo are found.",
      "oneOf": [
        {
          "description": "Allow seeding.",
          "type": "object",
          "properties": {
            "default": {
              "type": "string",
              "const": "allow"
            }
          },
          "anyOf": [
            {
              "$ref": "#/$defs/Scope"
            },
            {
              "type": "null"
            }
          ],
          "required": [
            "default"
          ]
        },
        {
          "description": "Block seeding.",
          "type": "object",
          "properties": {
            "default": {
              "type": "string",
              "const": "block"
            }
          },
          "required": [
            "default"
          ]
        }
      ]
    },
    "Scope": {
      "description": "Follow scope of a seeded repository.",
      "oneOf": [
        {
          "description": "Seed remotes that are explicitly followed.",
          "type": "string",
          "const": "followed"
        },
        {
          "description": "Seed all remotes.",
          "type": "string",
          "const": "all"
        }
      ]
    },
    "Config": {
      "description": "Database configuration.",
      "type": "object",
      "properties": {
        "sqlite": {
          "description": "SQLite configuration.",
          "$ref": "#/$defs/SqliteConfig"
        }
      },
      "required": [
        "sqlite"
      ]
    },
    "SqliteConfig": {
      "description": "SQLite database configuration.",
      "type": "object",
      "properties": {
        "pragma": {
          "$ref": "#/$defs/Pragma"
        }
      }
    },
    "Pragma": {
      "description": "Global SQLite pragma statements to make in order to configure SQLite itself,/nsee <https://sqlite.org/pragma.html>.",
      "type": "object",
      "properties": {
        "journalMode": {
          "$ref": "#/$defs/JournalMode"
        },
        "synchronous": {
          "$ref": "#/$defs/Synchronous"
        }
      }
    },
    "JournalMode": {
      "description": "Value for a `journal_mode` pragma statement./nFor a description of all variants please refer to/n<https://sqlite.org/pragma.html#pragma_journal_mode>./nNote that when SQLite documentation talks about /"the application/",/nthe application linked against this crate, e.g. Radicle Node, Radicle CLI,/nand others, is meant.",
      "type": "string",
      "enum": [
        "DELETE",
        "TRUNCATE",
        "PERSIST",
        "MEMORY",
        "WAL",
        "OFF"
      ]
    },
    "Synchronous": {
      "description": "Value for a `synchronous` pragma statement./nFor a description of all variants please refer to/n<https://sqlite.org/pragma.html#pragma_synchronous>.",
      "type": "string",
      "enum": [
        "EXTRA",
        "FULL",
        "NORMAL",
        "OFF"
      ]
    },
    "Fetch": {
      "description": "Configuration for fetching repositories from/nother nodes.",
      "type": "object",
      "properties": {
        "signedReferences": {
          "$ref": "#/$defs/SignedReferencesConfig"
        }
      }
    },
    "SignedReferencesConfig": {
      "type": "object",
      "properties": {
        "featureLevel": {
          "$ref": "#/$defs/FeatureLevelConfig"
        }
      }
    },
    "FeatureLevelConfig": {
      "type": "object",
      "properties": {
        "minimum": {
          "description": "The minimum feature level required to accept incoming/nreferences from other users. This value is compared/nagainst the feature level detected on refs as they are/nfetched./n/nNote that by increasing this value, security can be/ntraded for compatibility. The higher the value,/nthe less backward compatible, but the more secure, fetches will be.",
          "$ref": "#/$defs/FeatureLevel"
        }
      }
    },
    "FeatureLevel": {
      "description": "The Signed References feature has evolved over time./nThis enum captures the corresponding /"feature level/"./n/nFeature levels are monotonic, in the sense that a greater feature level/nencompasses all the features of smaller ones.",
      "oneOf": [
        {
          "description": "The lowest feature level, with least security. It is vulnerable to/ngraft attacks and replay attacks.",
          "type": "string",
          "const": "none"
        },
        {
          "description": "An intermediate feature level, which protects against graft attacks but is vulnerable to replay attacks. Introduced in Radicle 1.1.0, in commit `989edacd564fa658358f5ccfd08c243c5ebd8cda`.",
          "type": "string",
          "const": "root"
        },
        {
          "description": "The highest feature level known, which protects against graft attacks and replay attacks. Introduced in Radicle 1.7.0, in commit `d3bc868e84c334f113806df1737f52cc57c5453d`.",
          "type": "string",
          "const": "parent"
        }
      ]
    }
  }
}
```

You can also get any value in the configuration by path, eg.

```
$ rad config get node.alias
alice
$ rad config get preferredSeeds
z6MkrLMMsiPWUcNPHcRajuMi9mDfYckSoJyPwwnknocNYPm7@iris.radicle.network:8776
$ rad config get node.limits.routingMaxSize
1000
```

You can set scalar values by path.

```
$ rad config set node.alias bob
bob
$ rad config get node.alias
bob
```

You can push a value to a collection by path.

```
$ rad config push web.pinned.repositories rad:z3TajuiHXifEDEX4qbJxe8nXr9ufi
rad:z3TajuiHXifEDEX4qbJxe8nXr9ufi
$ rad config push web.pinned.repositories rad:z3trNYnLWS11cJWC6BbxDs5niGo82
rad:z3TajuiHXifEDEX4qbJxe8nXr9ufi
rad:z3trNYnLWS11cJWC6BbxDs5niGo82
```

You can remove a value from a collection by path.

```
$ rad config remove web.pinned.repositories rad:z3TajuiHXifEDEX4qbJxe8nXr9ufi
rad:z3trNYnLWS11cJWC6BbxDs5niGo82
```

Values that are not strictly required for a working configuration, such as
optional values or additional user-defined values, can be deleted.

```
$ rad config set web.name alice
alice
$ rad config unset web.name
```

``` (fail)
$ rad config get web.name
✗ Error: web.name does not exist in configuration found at "[..]/.radicle/config.json"
```

Values along the path will be created if necessary.

```
$ rad config set value.a.future.update.might.add.value 5
5
$ rad config push value.a.future.update.might.add.collection 1
1
```

```
$ rad config push node.array a
a
$ rad config push node.array b
a
b
```

Values that are required for a valid config can't be deleted.

``` (fail)
$ rad config unset node.alias
✗ Error: writing configuration to "[..]/.radicle/config.json" failed: validation failure due to missing field `alias`
```

Values for changes are being validated.

``` (fail)
$ rad config set web.pinned.repositories 5
✗ Error: writing configuration to "[..]/.radicle/config.json" failed: validation failure due to invalid type: integer `5`, expected a set
```

The type of the operation is validated.

``` (fail)
$ rad config push node.alias eve
✗ Error: failed to modify configuration found at "[..]/.radicle/config.json" due to the element at the path 'node.alias' is not a JSON array
```
