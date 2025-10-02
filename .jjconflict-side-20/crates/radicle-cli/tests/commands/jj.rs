use crate::util::environment::Environment;
use crate::{program_reports_version, test};
use radicle::git;

#[ignore = "the bare repository does not have a `rad` remote, and so it cannot determine the RID of the repository"]
#[test]
fn rad_jj_bare() {
    // We test whether `jj` is installed, and have this test succeed if it is not.
    // Programmatic skipping of tests is not supported as of 2024-08.
    if !program_reports_version("jj") {
        return;
    }

    let mut environment = Environment::new();
    let mut profile = environment.node("alice");
    let rid = profile.project("heartwood", "Radicle Heartwood Protocol & Stack");

    test(
        "examples/rad-init-existing-bare.md",
        environment.work(&profile),
        Some(&profile.home),
        [(
            "URL",
            git::url::File::new(profile.storage.path())
                .rid(rid)
                .to_string()
                .as_str(),
        )],
    )
    .unwrap();

    environment
        .tests(["jj-config", "jj-init-bare"], &profile)
        .unwrap();
}

#[test]
fn rad_jj_colocated_patch() {
    // We test whether `jj` is installed, and have this test succeed if it is not.
    // Programmatic skipping of tests is not supported as of 2024-08.
    if !program_reports_version("jj") {
        return;
    }

    Environment::alice(["rad-init", "jj-config", "jj-init-colocate", "rad-patch-jj"])
}
