We want to ensure that a warning is printed when the `scope` field is missing in the `seedingPolicy`.

``` alice
$ rad node status
! Warning: Configuration option 'node.seedingPolicy.scope' is not set, and thus takes the value 'all' by default. The default value will change to 'followed' in a future release. Please edit your configuration file, and set it to one of ['all', 'followed'] explicitly.
[..]
```
