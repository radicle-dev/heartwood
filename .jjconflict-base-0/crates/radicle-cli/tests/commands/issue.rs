use crate::util::environment::Environment;

#[test]
fn rad_issue() {
    Environment::alice(["rad-init", "rad-issue"]);
}

#[test]
fn rad_issue_list() {
    Environment::alice(["rad-init", "rad-issue", "rad-issue-list"]);
}
