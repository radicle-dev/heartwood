DID is initialized already because we depend on `rad-did-init.md`.

Suppose our signing key is compromised.

```
$ rad did panic
Stay calm.

What happened?

 1. Access to a secret key was compromised.
 2. Access to a secret key was lost.
 3. None of the above.

> 1

Access to which secret key was compromised?

 1. Controlling Key: did:key:z[..]
 2. Signing Key: did:key:z[..]

> 2.

You should:
 1. Generate a new signing key.
 2. Revoke the compromised signing key did:key:z[..]
 3. Rotate in the new signing key.

Do you want to proceed?
> y

✓ Creating your Ed25519 signing keypair...

  Signing Keys:
    Public: did:key:z6MK[..]
            (see also ~/.radicle/did/[..]/sign/1.pub)
    Secret: ~/.radicle/did/[..]/sign/1
  
✓ Revoking your compromised signing key...
✓ Rotating in your signing key...
```

The user thinks they lost access to a key, but actually did not:

```
$ did rad panic

What happened?

 1. Access to a secret key was compromised.
 2. Access to a secret key was lost.
 3. None of the above.

> 2

Access to which secret key was compromised?

 1. Controlling Key: did:key:z[..]
 2. Signing Key: did:key:z[..]

> 2.

Checking whether secret key can be accessed...

Successfully accessed your signing key. Access is not lost.

Please feel free to reach out for support via Zulip, if you are comfortable with that.

   <https://radicle.zulipchat.com/>
```

Revocation is also possible directly:

```
$ did rad revoke --reason=compromise did:key:...

```

The user does not know what happened, so ask other humans:

```
$ did rad panic

What happened?

 1. Access to a secret key was compromised.
 2. Access to a secret key was lost.
 3. None of the above.

> 3

If possible, please 

Please feel free to reach out for support via Zulip, if you are comfortable with that.

   <https://radicle.zulipchat.com/>
```