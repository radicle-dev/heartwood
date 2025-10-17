use proptest::{collection, prop_oneof, strategy::Strategy};
use radicle_git_metadata::commit::headers::Headers;

use crate::test::r#gen;

pub fn headers() -> impl Strategy<Value = Headers> {
    collection::vec(prop_oneof![header(), signature()], 0..5).prop_map(|hs| {
        let mut headers = Headers::new();
        for (k, v) in hs {
            headers.push(&k, &v);
        }
        headers
    })
}

fn header() -> impl Strategy<Value = (String, String)> {
    (prop_oneof!["test", "foo", "foobar"], r#gen::alphanumeric())
}

pub fn signature() -> impl Strategy<Value = (String, String)> {
    ("gpgsig", prop_oneof![pgp(), ssh()])
}

pub fn pgp() -> impl Strategy<Value = String> {
    "-----BEGIN PGP SIGNATURE-----\r?\n([A-Za-z0-9+/=\r\n]+)\r?\n-----END PGP SIGNATURE-----"
}

pub fn ssh() -> impl Strategy<Value = String> {
    "-----BEGIN SSH SIGNATURE-----\r?\n([A-Za-z0-9+/=\r\n]+)\r?\n-----END SSH SIGNATURE-----"
}
