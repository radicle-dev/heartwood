Note that aliases must not be longer than 32 bytes, or you will get an error.
There are other rules as well:

``` (stderr) (fail)
$ rad auth --alias "5fad63fe6b339fa92c588d926121bea6240773a7"
error: invalid value '5fad63fe6b339fa92c588d926121bea6240773a7' for '--alias <ALIAS>': alias cannot be greater than 32 bytes

For more information, try '--help'.
```

``` (stderr) (fail)
$ rad auth --alias "john doe"
error: invalid value 'john doe' for '--alias <ALIAS>': alias cannot contain whitespace or control characters

For more information, try '--help'.
```

``` (stderr) (fail)
$ rad auth --alias ""
error: invalid value '' for '--alias <ALIAS>': alias cannot be empty

For more information, try '--help'.
```
