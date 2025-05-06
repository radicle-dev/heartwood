```
list  list known DIDs, with status (cached, at which version?, private key found?)
      if there is an activated DID, it is printed first

init      initialize/incept a new DID
  [--from=<public-key>]
  [--from="\$(rad self --ssh-key)"] to bootstrap
  [--to=<public-key>] for pre-rotation (breaks bridging from self)

activate  activate ("login") a particular DID

cache     clear/refresh the DID cache, this could scan all seeded repos for DIDs
log       shows the key events of 

rotate  to rotate to a new (or the pre-rotated key)
revoke  
edit
sign    an arbitrary message using DID (we might use this for releases)
show
```

Initializing a new DID with `rad did`.

The example below is run with `RAD_DID_CONTROLLING_KEY_PASSPHRASE` set.


```
$ rad did init

Initializing your radicle 👾 identity

✓ Creating your identity repository...
✓ Your Radicle DID is did:rad:z[..]. This identifies you. Run `rad did` to show it at all times.

✓ Creating your Ed25519 controlling keypair...
✓ Signing inception event...

  Controlling Keys:
    Public: did:key:z6MK[..]
    Secret: ~/.radicle/did/[..]/control/0

✓ Creating your Ed25519 signing keypair...
✓ Rotating in your signing key...

  Signing Keys:
    Public: did:key:z6MK[..]
    Secret: ~/.radicle/did/[..]/sign/0

✓ You're all set.

✗ Hint: install ssh-agent to have it fill in your passphrase for you when signing.

To create a Radicle repository, run `rad init` from a Git repository with at least one commit.
To clone a repository, run `rad clone <rid>`. For example, `rad clone rad:z3gqcJUoA1n9HaHKufZs5FCSGazv5` clones the Radicle 'heartwood' repository.
To get a list of all commands, run `rad`.
```

You can get the above information at all times using the `self` command:


```
$ rad did list
did:key:z[..]
```

```
$ rad did
did:key:z[..]
```

You can also show your alias:

```
$ rad self --alias
alice
```
