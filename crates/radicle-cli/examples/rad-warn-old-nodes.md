```
$ rad config push preferredSeeds z6MkrLMMsiPWUcNPHcRajuMi9mDfYckSoJyPwwnknocNYPm7@seed.radicle.garden:8776
z6MkrLMMsiPWUcNPHcRajuMi9mDfYckSoJyPwwnknocNYPm7@seed.radicle.garden:8776
$ rad config push node.connect z6Mkmqogy2qEM2ummccUthFEaaHvyYmYBYh3dbe9W4ebScxo@ash.radicle.garden:8776
z6Mkmqogy2qEM2ummccUthFEaaHvyYmYBYh3dbe9W4ebScxo@ash.radicle.garden:8776
$ rad config push node.connect z6MkrLMMsiPWUcNPHcRajuMi9mDfYckSoJyPwwnknocNYPm7@iris.radicle.xyz:8776
z6Mkmqogy2qEM2ummccUthFEaaHvyYmYBYh3dbe9W4ebScxo@ash.radicle.garden:8776
z6MkrLMMsiPWUcNPHcRajuMi9mDfYckSoJyPwwnknocNYPm7@iris.radicle.xyz:8776
$ rad config push preferredSeeds z6Mkmqogy2qEM2ummccUthFEaaHvyYmYBYh3dbe9W4ebScxo@rosa.radicle.xyz:8776
z6MkrLMMsiPWUcNPHcRajuMi9mDfYckSoJyPwwnknocNYPm7@seed.radicle.garden:8776
z6Mkmqogy2qEM2ummccUthFEaaHvyYmYBYh3dbe9W4ebScxo@rosa.radicle.xyz:8776
```

Note the warnings that the above configuration causes when running `rad node status`:

```
$ rad node status
! Warning: Value of configuration option `node.connect` at index 0 mentions node with hostname 'ash.radicle.garden', which has been renamed to 'rosa.radicle.network'. Please edit your configuration file to use the new address.
! Warning: Value of configuration option `node.connect` at index 1 mentions node with hostname 'iris.radicle.xyz', which has been renamed to 'iris.radicle.network'. Please edit your configuration file to use the new address.
! Warning: Value of configuration option `preferredSeeds` at index 0 mentions node with hostname 'seed.radicle.garden', which has been renamed to 'iris.radicle.network'. Please edit your configuration file to use the new address.
! Warning: Value of configuration option `preferredSeeds` at index 1 mentions node with hostname 'rosa.radicle.xyz', which has been renamed to 'rosa.radicle.network'. Please edit your configuration file to use the new address.
Node is stopped.
To start it, run `rad node start`.
```
