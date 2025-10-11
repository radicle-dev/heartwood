The `rad self` command is used to display information about your local
device and node.

```
$ rad self
Alias        alice
DID          did:key:z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi
Node         not running
SSH          not running
├╴Key (hash) SHA256:UIedaL6Cxm6OUErh9GQUzzglSk7VpQlVTI1TAFB/HWA
└╴Key (full) ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIHahWSBEpuT1ESZbynOmBNkLBSnR32Ar4woZqSV2YNH1
Home         [..]/alice/.radicle
├╴Config     [..]/alice/.radicle/config.json
├╴Storage    [..]/alice/.radicle/storage
├╴Keys       [..]/alice/.radicle/keys
└╴Node       [..]/alice/.radicle/node
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

``` (stderr)
$ rad self --nid
! Deprecated: The command/option `rad self --nid` is deprecated and will be removed. Please use `rad node status --only nid` instead.
```

```
$ rad self --ssh-key
ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIHahWSBEpuT1ESZbynOmBNkLBSnR32Ar4woZqSV2YNH1
```

```
$ rad self --home
[..]/alice/.radicle
```
