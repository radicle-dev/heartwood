We want to ensure that a warning is printed when the `scope` field is missing in the `seedingPolicy`.

``` alice
$ rad node status
! Warning: node 'seedingPolicy.scope' has been set to 'all' by default. This default value will be removed in a future release. Please explicitly set it to one of ['all', 'followed'] in your node config.
[..]
```
