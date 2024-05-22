This example demonstrates handling of arbitrary Collaborative Objects (COBs) by means of an external program.
The external program is called a "COB helper" (analogous to "remote helpers" that can be used to extend Git).
It consumes the current state of the COB and at least one operation (all in [JSON]) to be applied to it via files whose names are passed as arguments.
It returns the resulting COB by printing it to standard output in [JSON].

For the sake of example, consider a simplified shopping list, where every item on the list is associated with an integral quantity:

 - 5 Bananas
 - 3 Zucchini
 - 1 Bar of Chocolate

One data structure that maps to this concept very well is a [multiset] (sometimes also called "bag").
So, let us introduce a new COB type with the name `com.example.multiset` that implements multisets, which we will then use to model our shopping list.

The COB we implement should allow two actions:

 - The action `+`, which adds an item or, if already present, increases the associated quantity.
 - The action `-`, which decreases the associated quantity of an item, if it is non-zero.

We model actions as objects in [JSON], and a sequence of actions in [JSON Lines].
An example sequence of actions looks as follows:

``` ./groceries.jsonl
{ "+": "jelly" }
{ "+": "peanut butter" }
{ "-": "jelly" }
{ "-": "jelly" }
{ "+": "salad" }
{ "+": "salad" }
```

Starting with an empty grocery list, the expected result after evaluating all actions is:

 - 0 Jelly (this could be omitted)
 - 1 Peanut Butter
 - 2 Salad

We have a COB helper, named `rad-cob-multiset`, that implements evaluation of these actions using [jq].
It reads the current state of the grocery list and operations containing actions from files given as arguments and writes the resulting grocery list to standard output.

We do not invoke the program directly, but instead use `rad cob create`:

```
$ rad cob create --repo rad:z42hL2jL4XNk6K8oHQaSWfMgCL7ji --type com.example.multiset --message "Create grocery shopping multiset" groceries.jsonl
9bba8e6f83ef56b11151ef6ad02cc4595f982aab
```

We can verify that the COB evaluated as expected:

```
$ rad cob show --repo rad:z42hL2jL4XNk6K8oHQaSWfMgCL7ji --type com.example.multiset --object 9bba8e6f83ef56b11151ef6ad02cc4595f982aab
{"jelly":0,"peanut butter":1,"salad":2}
```

To apply actions to COBs that already exist, we can use `rad cob update`:

```
$ rad cob update --repo rad:z42hL2jL4XNk6K8oHQaSWfMgCL7ji --type com.example.multiset --object 9bba8e6f83ef56b11151ef6ad02cc4595f982aab --message "Modify grocery shopping multiset" groceries.jsonl
d36aac77be13c1ca80edbfe7b7bf9b42c723f019
```

Again, we verify the result with `rad cob show`:

```
$ rad cob show --repo rad:z42hL2jL4XNk6K8oHQaSWfMgCL7ji --type com.example.multiset --object 9bba8e6f83ef56b11151ef6ad02cc4595f982aab
{"jelly":0,"peanut butter":2,"salad":4}
```

[multisets]: https://wikipedia.org/wiki/Multiset
[JSON]: https://tools.ietf.org/html/std90
[JSON Lines]: https://jsonlines.org/
[jq]: https://github.com/jqlang/jq