use std::fmt::Debug;

#[cfg(feature = "serde")]
pub fn json<A>(a: A)
where
    for<'de> A: Debug + PartialEq + serde::Serialize + serde::Deserialize<'de>,
{
    assert_eq!(
        a,
        serde_json::from_str(&serde_json::to_string(&a).unwrap()).unwrap()
    )
}
