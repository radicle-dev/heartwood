This test assumes that one of the two keys in `$RAD_HOME/keys` was swapped so that `$RAD_HOME/keys/radicle{,.pub}` do not match anymore.

``` (fail)
$ rad issue open --title "flux capacitor underpowered" --description "Flux capacitor power requirements exceed current supply" --no-announce
✗ Error: secret key '[..]/.radicle/keys/radicle' and public key '[..]/.radicle/keys/radicle.pub' do not match
```