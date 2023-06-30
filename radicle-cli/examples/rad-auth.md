Initializing a new identity with `rad-auth`.
The example below is run with `RAD_PASSPHRASE` set.

```
$ rad auth

Initializing your radicle 👾 identity

✓ Creating your Ed25519 keypair...
✓ Your Radicle DID is did:key:z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi. This identifies your device.

To create a radicle project, run `rad init` from a git repository.
```

You can get the above information at all times using the `self` command:

```
$ rad self --did
did:key:z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi
```
