Seed has an allow policy that is restricted to 'followed' scope.

Bob has a repository.

``` ~bob
$ rad ls
╭──────────────────────────────────────────────────────────────────────────────────────────────────────────╮
│ Name        RID                                Visibility   Head      Description                        │
├──────────────────────────────────────────────────────────────────────────────────────────────────────────┤
│ heartwood   rad:z4DJ16cSfDMzPRw51tH4kP66E4rE   public       f2de534   Radicle Heartwood Protocol & Stack │
╰──────────────────────────────────────────────────────────────────────────────────────────────────────────╯
$ rad node status --only nid
z6Mkux1aUQD2voWWukVb5nNUR7thrHveQG4pDQua8nVhib7Z
```

Eve has another repository.

``` ~eve
$ rad ls
╭────────────────────────────────────────────────────────────────────────────────────────────╮
│ Name      RID                                 Visibility   Head      Description           │
├────────────────────────────────────────────────────────────────────────────────────────────┤
│ nixpkgs   rad:z3zTnCfi6cVSZG8eCGn6AMDypgAPm   public       f2de534   Home for Nix Packages │
╰────────────────────────────────────────────────────────────────────────────────────────────╯
$ rad node status --only nid
z6Mkt67GdsW7715MEfRuP4pSZxJRJh6kj6Y48WRqVv4N1tRk
```

Seed connects to Bob and Eve, and checks it's inventory for their respective repositories.

``` ~seed
$ rad node inventory --nid z6Mkux1aUQD2voWWukVb5nNUR7thrHveQG4pDQua8nVhib7Z
rad:z4DJ16cSfDMzPRw51tH4kP66E4rE
$ rad node inventory --nid z6Mkt67GdsW7715MEfRuP4pSZxJRJh6kj6Y48WRqVv4N1tRk
rad:z3zTnCfi6cVSZG8eCGn6AMDypgAPm
```

Eve clones Bob's repository and commits a change on some branch. She attempts to submit it as a patch to Seed:

``` ~eve
$ rad clone rad:z4DJ16cSfDMzPRw51tH4kP66E4rE
✓ Seeding policy updated for rad:z4DJ16cSfDMzPRw51tH4kP66E4rE with scope 'followed'
Fetching rad:z4DJ16cSfDMzPRw51tH4kP66E4rE from the network, found 1 potential seed(s).
✓ Target met: 1 seed(s)
✓ Creating checkout in ./heartwood..
✓ Remote z6Mkux1aUQD2voWWukVb5nNUR7thrHveQG4pDQua8nVhib7Z added
✓ Remote-tracking branch z6Mkux1aUQD2voWWukVb5nNUR7thrHveQG4pDQua8nVhib7Z/master created for z6Mkux1aUQD2voWWukVb5nNUR7thrHveQG4pDQua8nVhib7Z
✓ Repository successfully cloned under [..]/heartwood/
╭────────────────────────────────────╮
│ heartwood                          │
│ Radicle Heartwood Protocol & Stack │
│ 0 issues · 0 patches               │
╰────────────────────────────────────╯
Run `cd ./heartwood` to go to the repository directory.
$ cd heartwood
$ git checkout -b flux-capacitor-power
$ touch REQUIREMENTS
$ git add REQUIREMENTS
$ git commit -v -m "Define power requirements"
[flux-capacitor-power 3e674d1] Define power requirements
 1 file changed, 0 insertions(+), 0 deletions(-)
 create mode 100644 REQUIREMENTS
$ git push rad -o patch.message="Define power requirements" -o patch.message="See details." HEAD:refs/patches
```

Seed is only tracking delegate refs, so Eve is unable to push her patch to the network.

However Eve's patch will be listed locally as an open patch.

``` ~eve
$ rad patch
╭─────────────────────────────────────────────────────────────────────────────────────────╮
│ ●  ID       Title                      Author         Reviews  Head     +   -   Updated │
├─────────────────────────────────────────────────────────────────────────────────────────┤
│ ●  [..]  Define power requirements  eve     (you)  -        3e674d1  +0  -0  now     │
╰─────────────────────────────────────────────────────────────────────────────────────────╯
```
