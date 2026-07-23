use proptest::prelude::*;
use radicle_git_ext::ref_format::{RefStr, RefString};
use radicle_git_ext_test::git_ref_format::gen;
use radicle_surf::Branch;
use test_helpers::roundtrip;

proptest! {
    #[test]
    fn prop_test_branch(branch in gen_branch()) {
        roundtrip::json(branch)
    }
}

fn gen_branch() -> impl Strategy<Value = Branch> {
    prop_oneof![
        gen::valid().prop_map(|name| Branch::local(RefString::try_from(name).unwrap())),
        (gen::valid(), gen::valid()).prop_map(|(remote, name): (String, String)| {
            let remote =
                RefStr::try_from_str(&remote).expect("BUG: reference strings should be valid");
            let name = RefStr::try_from_str(&name).expect("BUG: reference strings should be valid");
            Branch::remote(remote.head(), name)
        })
    ]
}
