Note the warnings that the configuration causes:

```
$ rad debug
{
  "radExe": "[..]",
  "radVersion": "[..]",
  "radicleNodeVersion": "radicle-node [..]",
  "gitRemoteRadVersion": "git-remote-rad [..]",
  "gitVersion": "git version [..]",
  "sshVersion": "[..]",
  "gitHead": "[..]",
  "log": {
    "filename": "[..]",
    "exists": false,
    "len": null
  },
  "oldLog": {
    "filename": "[..]",
    "exists": false,
    "len": null
  },
  "operatingSystem": "[..]",
  "arch": "[..]",
  "env": {
    "PATH": "[..]",
    "RAD_HOME": "[..]",
    "RAD_KEYGEN_SEED": "ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff",
    "RAD_LOCAL_TIME": "[..]",
    "RAD_PASSPHRASE": "<REDACTED>",
    "RAD_RNG_SEED": "0"
  },
  "warnings": [
    "Value of configuration option `node.connect` at index 0 mentions node with hostname 'ash.radicle.garden', which has been renamed to 'rosa.radicle.network'. Please edit your configuration file to use the new address.",
    "Value of configuration option `node.connect` at index 1 mentions node with hostname 'iris.radicle.xyz', which has been renamed to 'iris.radicle.network'. Please edit your configuration file to use the new address.",
    "Value of configuration option `preferredSeeds` at index 0 mentions node with hostname 'seed.radicle.garden', which has been renamed to 'iris.radicle.network'. Please edit your configuration file to use the new address.",
    "Value of configuration option `preferredSeeds` at index 1 mentions node with hostname 'rosa.radicle.xyz', which has been renamed to 'rosa.radicle.network'. Please edit your configuration file to use the new address."
  ]
}
```

Also, `rad node status` will warn us:

```
$ rad node status
! Warning: Value of configuration option `node.connect` at index 0 mentions node with hostname 'ash.radicle.garden', which has been renamed to 'rosa.radicle.network'. Please edit your configuration file to use the new address.
! Warning: Value of configuration option `node.connect` at index 1 mentions node with hostname 'iris.radicle.xyz', which has been renamed to 'iris.radicle.network'. Please edit your configuration file to use the new address.
! Warning: Value of configuration option `preferredSeeds` at index 0 mentions node with hostname 'seed.radicle.garden', which has been renamed to 'iris.radicle.network'. Please edit your configuration file to use the new address.
! Warning: Value of configuration option `preferredSeeds` at index 1 mentions node with hostname 'rosa.radicle.xyz', which has been renamed to 'rosa.radicle.network'. Please edit your configuration file to use the new address.
Node is stopped.
To start it, run `rad node start`.
```
