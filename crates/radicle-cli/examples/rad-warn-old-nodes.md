Note the warnings that the configuration causes when running `rad node status`:

```
$ rad node status
! Warning: Value of configuration option `node.connect` at index 0 mentions node with hostname 'ash.radicle.garden', which has been renamed to 'rosa.radicle.network'. Please edit your configuration file to use the new address.
! Warning: Value of configuration option `node.connect` at index 1 mentions node with hostname 'iris.radicle.xyz', which has been renamed to 'iris.radicle.network'. Please edit your configuration file to use the new address.
! Warning: Value of configuration option `preferredSeeds` at index 0 mentions node with hostname 'seed.radicle.garden', which has been renamed to 'iris.radicle.network'. Please edit your configuration file to use the new address.
! Warning: Value of configuration option `preferredSeeds` at index 1 mentions node with hostname 'rosa.radicle.xyz', which has been renamed to 'rosa.radicle.network'. Please edit your configuration file to use the new address.
Node is stopped.
To start it, run `rad node start`.
```
