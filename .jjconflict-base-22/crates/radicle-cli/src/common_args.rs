use std::str::FromStr;

use radicle::storage::refs;

pub const ABOUT_FETCH_SIGNED_REFERENCES_FEATURE_LEVEL_MINIMUM: &str = r#"
Perform the fetch using the provided Signed References minimum feature
level.

The options for this value provide the following behavior:

`parent`: Only the namespaces that use the parent feature level will be
accepted. This prevents graft attacks and replay attacks from
occurring. This provides the most security.

`root`: Only the namespaces that use the root feature level will be
accepted. This prevents graft attacks from occurring. This option
should be only used if you trust the node you are fetching from, and
want to bypass security for backwards compatibility.

`none`: All namespaces will be fetched regardless of feature level
detected, and provides no security against graft attacks or replay
attacks. This option should be only used if you trust the node you are
fetching from, and want to bypass security for backwards compatibility.
"#;

#[derive(Clone, Copy, Debug)]
pub struct SignedReferencesFeatureLevel {
    inner: refs::FeatureLevel,
}

impl From<SignedReferencesFeatureLevel> for refs::FeatureLevel {
    fn from(SignedReferencesFeatureLevel { inner }: SignedReferencesFeatureLevel) -> Self {
        inner
    }
}

impl FromStr for SignedReferencesFeatureLevel {
    type Err = &'static str;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let inner = match s {
            "none" => refs::FeatureLevel::None,
            "root" => refs::FeatureLevel::Root,
            "parent" => refs::FeatureLevel::Parent,
            _ => return Err("invalid feature level"),
        };
        Ok(Self { inner })
    }
}

#[derive(Clone, Debug)]
pub struct SignedReferencesFeatureLevelParser;

impl clap::builder::TypedValueParser for SignedReferencesFeatureLevelParser {
    type Value = SignedReferencesFeatureLevel;

    fn parse_ref(
        &self,
        cmd: &clap::Command,
        arg: Option<&clap::Arg>,
        value: &std::ffi::OsStr,
    ) -> Result<Self::Value, clap::Error> {
        <SignedReferencesFeatureLevel as std::str::FromStr>::from_str.parse_ref(cmd, arg, value)
    }

    fn possible_values(
        &self,
    ) -> Option<Box<dyn Iterator<Item = clap::builder::PossibleValue> + '_>> {
        use clap::builder::PossibleValue;
        Some(Box::new(
            [
                PossibleValue::new("parent"),
                PossibleValue::new("root"),
                PossibleValue::new("none"),
            ]
            .into_iter(),
        ))
    }
}
