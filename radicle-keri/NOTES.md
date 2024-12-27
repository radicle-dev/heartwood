
Trait for `EventDatabase` inspired by `SledEventDatabase` with another impl based on a Git repository (JSON files?!).
Repeat for `EventValidator`.


```rust
pub trait keri_core::signer::KeyManager {
    fn sign(&self, msg: &[u8]) -> Result<Vec<u8>, Error>;
    fn public_key(&self) -> PublicKey;
    fn next_public_key(&self) -> PublicKey;
    fn rotate(&mut self) -> Result<(), Error>;
}
```

Keep `keri_core::state::IdentifierState`.