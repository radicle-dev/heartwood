The `rad self` command is used to display information about your local
device and node.

```
$ rad self
DID            did:key:z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi
Node ID (NID)  z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi
Key (hash)     SHA256:UIedaL6Cxm6OUErh9GQUzzglSk7VpQlVTI1TAFB/HWA
Key (full)     ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIHahWSBEpuT1ESZbynOmBNkLBSnR32Ar4woZqSV2YNH1
Storage (git)  [..]
Storage (keys) [..]
Node (socket)  [..]
```

If you need to display only your DID, Node ID, or SSH Public Key, you can use
the various options available:

```
$ rad self --did
did:key:z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi
```

```
$ rad self --nid
z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi
```

```
$ rad self --ssh-key
ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIHahWSBEpuT1ESZbynOmBNkLBSnR32Ar4woZqSV2YNH1
```
