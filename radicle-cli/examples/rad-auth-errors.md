Note that aliases must not be longer than 32 bytes, or you will get an error.
There are other rules as well:

``` (fail)
$ rad auth --alias "5fad63fe6b339fa92c588d926121bea6240773a7"
✗ Error: rad auth: alias cannot be greater than 32 bytes
```

``` (fail)
$ rad auth --alias "john doe"
✗ Error: rad auth: alias cannot contain whitespace or control characters
```

``` (fail)
$ rad auth --alias ""
✗ Error: rad auth: alias cannot be empty
```
