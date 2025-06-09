# radicle-cli-test

Test your CLI with the help of markdown descriptions.

## Example

Test flows are described in markdown like this example:

````` markdown
# Echoing works

When I call echo, it answers:

```
$ echo "ohai"
ohai
```
`````

Say this is placed in `kind-echo.md`, this is what the corresponding test case
would look lke:

``` rust
use std::path::Path;
use radicle_cli_test::TestFormula;

#[test]
fn kind_echo() {
    TestFormula::new()
        .file(Path::new("./kind-echo.md"))
        .unwrap()
        .run()
        .unwrap();
}
```
