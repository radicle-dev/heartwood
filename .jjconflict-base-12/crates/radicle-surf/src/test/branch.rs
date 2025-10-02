use super::r#gen;
use crate::Branch;
use proptest::prelude::*;
use radicle_git_ref_format::{RefStr, RefString};

proptest! {
    #[test]
    fn prop_test_branch(branch in gen_branch()) {
        super::roundtrip::json(branch)
    }
}

fn gen_branch() -> impl Strategy<Value = Branch> {
    prop_oneof![
        r#gen::valid().prop_map(|name| Branch::local(RefString::try_from(name).unwrap())),
        (r#gen::valid(), r#gen::valid()).prop_map(|(remote, name): (String, String)| {
            let remote =
                RefStr::try_from_str(&remote).expect("BUG: reference strings should be valid");
            let name = RefStr::try_from_str(&name).expect("BUG: reference strings should be valid");
            Branch::remote(remote.head(), name)
        })
    ]
}
