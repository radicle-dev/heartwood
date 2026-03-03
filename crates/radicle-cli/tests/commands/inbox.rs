use crate::util::environment::Environment;
use crate::util::formula::formula;
use radicle::test::fixtures;

#[test]
fn rad_inbox() {
    let mut environment = Environment::new();
    let mut alice = environment.node("alice");
    let bob = environment.node("bob");
    let (repo1, _) = fixtures::repository(environment.work(&alice).join("heartwood"));
    let (repo2, _) = fixtures::repository(environment.work(&alice).join("radicle-git"));
    let rid1 = alice.project_from("heartwood", "Radicle Heartwood Protocol & Stack", &repo1);
    let rid2 = alice.project_from("radicle-git", "Radicle Git", &repo2);

    let alice = alice.spawn();
    let mut bob = bob.spawn();

    bob.connect(&alice).converge([&alice]);
    bob.clone(rid1, environment.work(&bob)).unwrap();
    bob.clone(rid2, environment.work(&bob)).unwrap();

    formula(&environment.tempdir(), "examples/rad-inbox.md")
        .unwrap()
        .home(
            "alice",
            environment.work(&alice),
            [("RAD_HOME", alice.home.path().display())],
        )
        .home(
            "bob",
            environment.work(&bob),
            [("RAD_HOME", bob.home.path().display())],
        )
        .run()
        .unwrap();
}
