```
$ rad config push preferredSeeds z6MkrLMMsiPWUcNPHcRajuMi9mDfYckSoJyPwwnknocNYPm7@seed.radicle.garden:8776
z6MkrLMMsiPWUcNPHcRajuMi9mDfYckSoJyPwwnknocNYPm7@seed.radicle.garden:8776
$ rad config push node.connect z6Mkmqogy2qEM2ummccUthFEaaHvyYmYBYh3dbe9W4ebScxo@ash.radicle.garden:8776
z6Mkmqogy2qEM2ummccUthFEaaHvyYmYBYh3dbe9W4ebScxo@ash.radicle.garden:8776
```

Note the warnings that the above configuration causes:

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
    "Value of configuration option `node.connect` at index 0 mentions node with address 'ash.radicle.garden:8776', which has been renamed to 'rosa.radicle.xyz:8776'. Please update your configuration.",
    "Value of configuration option `preferred_seeds` at index 0 mentions node with address 'seed.radicle.garden:8776', which has been renamed to 'iris.radicle.xyz:8776'. Please update your configuration."
  ]
}
```

Also, `rad node status` will warn us:

```
$ rad node status
! Warning: Value of configuration option `node.connect` at index 0 mentions node with address 'ash.radicle.garden:8776', which has been renamed to 'rosa.radicle.xyz:8776'. Please update your configuration.
! Warning: Value of configuration option `preferred_seeds` at index 0 mentions node with address 'seed.radicle.garden:8776', which has been renamed to 'iris.radicle.xyz:8776'. Please update your configuration.
Node is stopped.
To start it, run `rad node start`.
```