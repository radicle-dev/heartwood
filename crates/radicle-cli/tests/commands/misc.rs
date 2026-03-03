use crate::test;
use crate::util::environment::Environment;
use crate::util::formula::formula;
use radicle::test::fixtures;

#[test]
fn rad_help() {
    Environment::alice(["rad-help"]);
}

#[test]
fn rad_auth() {
    test("examples/rad-auth.md", std::path::Path::new("."), None, []).unwrap();
}

#[test]
fn rad_key_mismatch() {
    let mut environment = Environment::new();
    let alice = environment.profile("alice");
    environment.repository(&alice);

    environment.test("rad-init", &alice).unwrap();

    // Replace the public key with one that does not match the secret key anymore.
    std::fs::write(alice.home.path().join("keys").join("radicle.pub"), "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIE6Ul/D+P0I/Hl1JVOWGS8Z589us9FqKQXWv8OMOpKCh snakeoil\n").unwrap();

    environment.test("rad-key-mismatch", &alice).unwrap();
}

#[test]
fn rad_auth_errors() {
    test(
        "examples/rad-auth-errors.md",
        std::path::Path::new("."),
        None,
        [],
    )
    .unwrap();
}

#[cfg(unix)]
#[test]
fn rad_diff() {
    if std::env::consts::OS == "macos" {
        // macOS's `sed` requires an argument for `-i`, which we don't provide
        // in the example. Providing it makes the test fail on Linux.
        // Since this command is deprecated anyway, we just skip macOS.
        return;
    }

    let tmp = tempfile::tempdir().unwrap();

    fixtures::repository(&tmp);

    test("examples/rad-diff.md", tmp, None, []).unwrap();
}

#[test]
fn framework_home() {
    let mut environment = Environment::new();
    let alice = environment.node("alice");
    let bob = environment.node("bob");

    formula(&environment.tempdir(), "examples/framework/home.md")
        .unwrap()
        .home(
            "alice",
            alice.home.path(),
            [("RAD_HOME", alice.home.path().display())],
        )
        .home(
            "bob",
            bob.home.path(),
            [("RAD_HOME", bob.home.path().display())],
        )
        .run()
        .unwrap();
}

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
