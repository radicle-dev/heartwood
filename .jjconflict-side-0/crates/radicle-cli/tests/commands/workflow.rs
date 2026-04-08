use crate::test;
use crate::util::environment::Environment;

#[test]
fn rad_workflow() {
    let mut environment = Environment::new();
    let alice = environment.node("alice");
    let bob = environment.node("bob");

    environment.repository(&alice);

    environment.test("workflow/1-new-project", &alice).unwrap();

    let alice = alice.spawn();
    let mut bob = bob.spawn();

    bob.connect(&alice).converge([&alice]);

    environment.test("workflow/2-cloning", &bob).unwrap();

    test(
        "examples/workflow/3-issues.md",
        environment.work(&bob).join("heartwood"),
        Some(&bob.home),
        [],
    )
    .unwrap();

    test(
        "examples/workflow/4-patching-contributor.md",
        environment.work(&bob).join("heartwood"),
        Some(&bob.home),
        [],
    )
    .unwrap();

    test(
        "examples/workflow/5-patching-maintainer.md",
        environment.work(&alice),
        Some(&alice.home),
        [],
    )
    .unwrap();

    test(
        "examples/workflow/6-pulling-contributor.md",
        environment.work(&bob).join("heartwood"),
        Some(&bob.home),
        [],
    )
    .unwrap();
}
