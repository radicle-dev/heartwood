Initializing a new identity with `rad-auth`.
The example below is run with `RAD_PASSPHRASE` set.

```
$ rad auth --alias "alice"

Initializing your radicle 👾 identity

✓ Creating your Ed25519 keypair...
✓ Your Radicle DID is did:key:z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi. This identifies your device. Run `rad self` to show it at all times.
✓ You're all set.

To create a Radicle repository, run `rad init` from a Git repository with at least one commit.
To clone a repository, run `rad clone <rid>`. For example, `rad clone rad:z3gqcJUoA1n9HaHKufZs5FCSGazv5` clones the Radicle 'heartwood' repository.
To get a list of all commands, run `rad help`.
```

You can get the above information at all times using the `self` command:

```
$ rad self --did
did:key:z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi
```

You can also show your alias:
```
$ rad self --alias
alice
```
