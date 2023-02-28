Initializing a new identity with `rad-auth`.
The example below is run with `RAD_PASSPHRASE` set.

```
$ rad auth

Initializing your 🌱 profile and identity

✓ Creating your 🌱 Ed25519 keypair...
! Adding your radicle key to ssh-agent...
✓ Your Radicle ID is did:key:z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi. This identifies your device.

👉 To create a radicle project, run `rad init` from a git repository.
```

You can get the above information at all times using the `self` command:

```
$ rad self
ID             did:key:z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi
Node ID        z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi
Key (hash)     SHA256:UIedaL6Cxm6OUErh9GQUzzglSk7VpQlVTI1TAFB/HWA
Key (full)     ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIHahWSBEpuT1ESZbynOmBNkLBSnR32Ar4woZqSV2YNH1
Storage (git)  [..]/storage
Storage (keys) [..]/keys
Node (socket)  [..]/node/radicle.sock
```
