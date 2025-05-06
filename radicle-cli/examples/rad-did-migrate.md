Initializing a new identity with `rad-auth`.
The example below is run with `RAD_PASSPHRASE` set.

```
$ rad auth --alias "alice"

Initializing your radicle 👾 identity

✓ Creating your Ed25519 keypair...
✓ Your Radicle DID is did:key:z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi. This identifies your device. Run `rad self` to show it at all times.
✓ You're all set.

✗ Hint: install ssh-agent to have it fill in your passphrase for you when signing.

To create a Radicle repository, run `rad init` from a Git repository with at least one commit.
To clone a repository, run `rad clone <rid>`. For example, `rad clone rad:z3gqcJUoA1n9HaHKufZs5FCSGazv5` clones the Radicle 'heartwood' repository.
To get a list of all commands, run `rad`.
```

Now, we migrate to `did:rad`.

```
$ rad did migrate --from=self

Initializing your radicle 👾 identity

✓ Creating your identity repository...
✓ Your Radicle DID is did:rad:z[..]. This identifies you. Run `rad did` to show it at all times.

✓ Creating your Ed25519 controlling keypair...
✓ Signing inception event...

  Controlling Keys:
    Public: did:key:z6MK[..]
   	    (see also ~/.radicle/did/[..]/control/0.pub)
    Secret: ~/.radicle/did/[..]/control/0

✓ Using your ...

  Signing Keys:
    Public: did:key:z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi
            ~/.radicle/did/[..]/sign/0.pub
            (copied from ~/.radicle/keys/radicle.pub)
    Secret: ~/.radicle/did/[..]/sign/0
            (copied from ~/.radicle/keys/radicle)

✓ You're all set.

✗ Hint: install ssh-agent to have it fill in your passphrase for you when signing.

To create a Radicle repository, run `rad init` from a Git repository with at least one commit.
To clone a repository, run `rad clone <rid>`. For example, `rad clone rad:z3gqcJUoA1n9HaHKufZs5FCSGazv5` clones the Radicle 'heartwood' repository.
To get a list of all commands, run `rad`.
```

You can get the above information at all times using the `rad did` command:

```
$ rad did
did:key:z[..]
```

.. unless the DID is deactivated, that is.

```
$ rad did deactivate

```

You can also show your alias:

```
$ rad self --alias
alice
```
