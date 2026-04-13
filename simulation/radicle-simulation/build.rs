use std::env;
use std::fs;
use std::num::NonZeroU64;
use std::path::Path as StdPath;
use std::process::Command;

// Include the shared constants
include!("src/constants.rs");

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
struct Version {
    major: u32,
    minor: u32,
    patch: u32,
}

impl Version {
    /// Helper to parse "peer-v1-5-0" into a `Version { 1, 5, 0 }` for sorting
    fn parse(name: &str) -> Version {
        let parts: Vec<&str> = name.split('-').collect();
        if parts.len() >= 4 && parts[0] == "peer" && parts[1].starts_with('v') {
            let major = parts[1][1..].parse().unwrap_or(0);
            let minor = parts[2].parse().unwrap_or(0);
            let patch = parts[3].parse().unwrap_or(0);
            Version {
                major,
                minor,
                patch,
            }
        } else {
            Version::default()
        }
    }
}

/// Helper to generate the `radicle_simulation::node::Node` path
fn node_type_path() -> ruast::Path {
    ruast::Path::new(vec![
        ruast::PathSegment::simple("radicle_simulation"),
        ruast::PathSegment::simple("node"),
        ruast::PathSegment::simple("Node"),
    ])
}

/// Helper to generate the `radicle_simulation::node::Node::new` path
fn node_new_path() -> ruast::Path {
    node_type_path().chain(ruast::PathSegment::simple("new"))
}

/// Helper to build the AST for `format!("{}-{}", node_name, index)`
///
/// This returns a function that is curried with `node_name` and expects `index`
/// as its next input parameter.
fn build_format_macro(node_name: &str) -> ruast::Expr {
    let mut ts = ruast::TokenStream::new();
    ts.extend(ruast::TokenStream::from(ruast::Expr::new(ruast::Lit::str(
        format!("{}-{{}}", node_name),
    ))));
    ts.push(ruast::Token::Comma);
    ts.extend(ruast::TokenStream::from(ruast::Expr::new(
        ruast::Path::single("index"),
    )));

    ruast::Expr::new(ruast::MacCall::new(
        ruast::Path::single("format"),
        ruast::DelimArgs::parenthesis(ts),
    ))
}

/// Generates the AST for an individual node function.
/// The function name is take from the `node_name` parameter.
/// If `replicas == 1`, then no `index` parameter is generated.
/// However, if `replicas > 1`, then an `index` parameter is used to allow the
/// generation of multiple `Node`'s with a `-{index}` suffix.
///
/// For example, resulting functions would look like:
/// ```
/// pub fn peer_v1_6_1() -> radicle_simulation::node::Node {
///    radicle_simulation::node::Node::new("peer-v1-6-1-0")
/// }
/// pub fn peer_v1_7_0(index: usize) -> radicle_simulation::node::Node {
///    radicle_simulation::node::Node::new(format!("peer-v1-7-0-{}" , index))
/// }
/// pub fn peer_v1_7_1(index: usize) -> radicle_simulation::node::Node {
///     radicle_simulation::node::Node::new(format!("peer-v1-7-1-{}" , index))
/// }
/// ```
fn generate_node_function(node_name: &str, replicas: NonZeroU64) -> ruast::Item {
    let fn_name = node_name.replace("-", "_");
    let mut base_fn = ruast::Fn::empty(fn_name);

    base_fn
        .fn_decl
        .set_output(ruast::Type::Path(node_type_path()));

    if replicas == NonZeroU64::MIN {
        base_fn.body = Some(ruast::Block::from(ruast::Expr::new(node_new_path()).call(
            vec![ruast::Expr::new(ruast::Lit::str(format!(
                "{}-0",
                node_name
            )))],
        )));
    } else {
        base_fn.fn_decl.add_input(ruast::Param::ident(
            "index",
            ruast::Type::Path(ruast::Path::single("usize")),
        ));

        let format_mac = build_format_macro(node_name);

        base_fn.body = Some(ruast::Block::from(
            ruast::Expr::new(node_new_path()).call(vec![format_mac]),
        ));
    }

    ruast::Item::public(base_fn)
}

// TODO(finto): The version offset could potentially be strange, e.g. we might
// release 2.0.0, 1.11.0, 2.1.0. The offset of -1 would be 1.11.0, but the
// intention might have been 2.0.0.
/// Generates the AST for the `peer_relative` function.
/// The function name is `peer_relative` and has two parameters: `offset` and `index`.
///
/// The `offset` parameter is expected to be `<= 0`, where a negative number
/// indicates how many versions behind, of Radicle, the node should be
/// initialized with. The maximum `offset` is calculated by the number of
/// `versioned_peers` passed in.
///
/// The `index` parameter gives the node a suffix `-{index}`.
///
/// The resulting generated function looks like:
///   `pub fn peer_relative(offset: isize, index: usize) -> radicle_simulation::node::Node`
///
/// The function body will include a `panic!` for an `offset` value that could
/// not be handled, i.e. exceeds the number of `versioned_peers`.
///
/// If there are no `versioned_peers`, a `panic!` will be emitted as the
/// function body.
fn generate_peer_relative_function(versioned_peers: &[(String, Version)]) -> ruast::Item {
    let len = versioned_peers.len() as isize;
    let mut peer_relative_fn = ruast::Fn::empty("peer_relative");

    peer_relative_fn.fn_decl.add_input(ruast::Param::ident(
        "offset",
        ruast::Type::Path(ruast::Path::single("isize")),
    ));
    peer_relative_fn.fn_decl.add_input(ruast::Param::ident(
        "index",
        ruast::Type::Path(ruast::Path::single("usize")),
    ));
    peer_relative_fn
        .fn_decl
        .set_output(ruast::Type::Path(node_type_path()));

    if len > 0 {
        let match_arms = versioned_peers
            .iter()
            .enumerate()
            .map(|(i, (node_name, _))| {
                let offset = (i as isize) - len + 1;
                let format_mac = build_format_macro(node_name);

                ruast::Arm::new(
                    ruast::Pat::Lit(ruast::Expr::new(ruast::Lit::int(offset.to_string()))),
                    None,
                    ruast::Expr::new(node_new_path()).call(vec![format_mac]),
                )
            })
            .chain(std::iter::once({
                let mut ts = ruast::TokenStream::new();
                ts.extend(ruast::TokenStream::from(ruast::Expr::new(ruast::Lit::str(
                    format!(
                        "Invalid relative offset. Available offsets: {}..=0",
                        1 - len
                    ),
                ))));

                let panic_mac = ruast::Expr::new(ruast::MacCall::new(
                    ruast::Path::single("panic"),
                    ruast::DelimArgs::parenthesis(ts),
                ));

                ruast::Arm::new(ruast::Pat::Wild, None, panic_mac)
            }))
            .collect::<Vec<_>>();

        peer_relative_fn.body = Some(ruast::Block::from(ruast::Expr::new(ruast::Match::new(
            ruast::Expr::new(ruast::Path::single("offset")),
            match_arms,
        ))));
    } else {
        let mut ts = ruast::TokenStream::new();
        ts.extend(ruast::TokenStream::from(ruast::Expr::new(ruast::Lit::str(
            "No versioned peers available",
        ))));

        let panic_mac = ruast::Expr::new(ruast::MacCall::new(
            ruast::Path::single("panic"),
            ruast::DelimArgs::parenthesis(ts),
        ));

        peer_relative_fn.body = Some(ruast::Block::from(panic_mac));
    };

    ruast::Item::public(peer_relative_fn)
}

/// Pure function to generate Rust bindings from a JSON string using `ruast`.
///
/// The final Rust bindings are the peers, if any, defined in the JSON (see
/// [`generate_node_function`]), and the `peer_relative` function (see
/// [`generate_peer_relative_function`])
///
/// # Examples
///
/// ```ignore
/// generate_bindings(json!({
///   "peer-v1-7-0": { "replicas": 1 },
///   "peer-v1-5-0": { "replicas": 1 },
///   "peer-v1-6-1": { "replicas": 1 }
/// }))
/// ```
///
/// ```ignore
/// generate_bindings(json!({}))
/// ```
fn generate_bindings(json_str: &str) -> Result<String, String> {
    let topology: serde_json::Value =
        serde_json::from_str(json_str).map_err(|e| format!("Invalid JSON: {}", e))?;

    let obj = topology.as_object().cloned().unwrap_or_default();

    let mut keys: Vec<String> = obj.keys().cloned().collect();
    keys.sort();

    // Generate the individual node functions
    let node_items = keys.iter().map(|node_name| {
        let config = &obj[node_name];
        let replicas = config
            .get("replicas")
            .and_then(|r| r.as_u64())
            .and_then(NonZeroU64::new)
            .unwrap_or(NonZeroU64::MIN);
        generate_node_function(node_name, replicas)
    });

    // Extract and sort versioned peers
    let mut versioned_peers: Vec<(String, Version)> = keys
        .iter()
        .filter(|name| name.starts_with("peer-v"))
        .map(|name| {
            let version = Version::parse(name);
            (name.clone(), version)
        })
        .collect();

    versioned_peers.sort_by_key(|k| k.1);

    // Generate the `peer_relative` function
    let peer_relative_fn = generate_peer_relative_function(&versioned_peers);

    // Assemble the final crate AST
    let mut krate = ruast::Crate::new();

    for item in node_items {
        krate.add_item(item);
    }
    krate.add_item(peer_relative_fn);

    Ok(krate.to_string())
}

/// Check if the provided `path` is a `.cue` file, and ensures that it is not
/// the `schema.cue` file.
fn is_valid_cue_instance(path: &StdPath) -> bool {
    let is_cue = path.extension().filter(|ext| *ext == "cue").is_some();
    let is_not_schema = path.file_name().and_then(|s| s.to_str()) != Some(SCHEMA_FILE);
    is_cue && is_not_schema
}

/// To inspect the generated Rust definitions, they can be found in:
/// `./target/debug/build/radicle-simulation-*/out/*_bindings.rs`.
#[allow(dead_code)]
fn main() {
    // Tell Cargo to recompile if anything in the instances directory changes.
    //
    // This may not trigger because of the top-level `Cargo.toml` containing
    // `default-members` that doesn't include `radicle-simulation` (so that we
    // don't run the simulation tests with a regular `cargo test`).
    //
    // You instead can use `$ cargo check -p radicle-simulation`
    println!("cargo:rerun-if-changed={}", INSTANCES_DIR);

    let out_dir = env::var_os("OUT_DIR").unwrap();
    let out_path = StdPath::new(&out_dir);

    let entries = fs::read_dir(INSTANCES_DIR).expect("Failed to read instances directory");

    for entry in entries {
        let entry = entry.expect("Failed to read directory entry");
        let path = entry.path();

        if path.is_file() && is_valid_cue_instance(&path) {
            let file_stem = path.file_stem().unwrap().to_str().unwrap();
            let cue_file = path.to_str().unwrap();

            // Export the topology block from CUE to JSON
            let output = Command::new("cue")
                .args([
                    "export",
                    cue_file,
                    SCHEMA_CUE_PATH,
                    "-e",
                    CUE_TOPOLOGY_PATH,
                    "--out",
                    "json",
                ])
                .output()
                .unwrap_or_else(|_| {
                    panic!(
                        "Failed to run `cue export` on {}. Ensure the CUE CLI is installed.",
                        cue_file
                    )
                });

            if !output.status.success() {
                panic!(
                    "Failed to export {}:\n{}",
                    cue_file,
                    String::from_utf8_lossy(&output.stderr)
                );
            }

            let json_str = String::from_utf8_lossy(&output.stdout);

            match generate_bindings(&json_str) {
                Ok(generated_code) => {
                    let dest_path = out_path.join(format!("{}_bindings.rs", file_stem));
                    fs::write(&dest_path, generated_code).unwrap();
                }
                Err(e) => {
                    println!(
                        "cargo:warning=Failed to generate bindings for {}: {}",
                        cue_file, e
                    );
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_version() {
        assert_eq!(
            Version::parse("peer-v1-5-0"),
            Version {
                major: 1,
                minor: 5,
                patch: 0
            }
        );
        assert_eq!(
            Version::parse("peer-v1-6-1"),
            Version {
                major: 1,
                minor: 6,
                patch: 1
            }
        );
        assert_eq!(
            Version::parse("peer-v2-0-0"),
            Version {
                major: 2,
                minor: 0,
                patch: 0
            }
        );

        assert_eq!(
            Version::parse("bootstrap-v1-6-1"),
            Version {
                major: 0,
                minor: 0,
                patch: 0
            }
        );
        assert_eq!(
            Version::parse("peer-latest"),
            Version {
                major: 0,
                minor: 0,
                patch: 0
            }
        );
        assert_eq!(
            Version::parse("random-string"),
            Version {
                major: 0,
                minor: 0,
                patch: 0
            }
        );
    }

    #[test]
    fn test_generate_bindings_empty() {
        let json = "{}";
        let code = generate_bindings(json).unwrap();
        assert!(code.contains("No versioned peers available"));
    }

    #[test]
    fn test_generate_bindings_single_replica() {
        let json = r#"{
            "peer-v1-5-0": {
                "replicas": 1
            }
        }"#;
        let code = generate_bindings(json).unwrap();
        let code_no_space = code.replace(" ", "");

        assert!(code_no_space.contains("pubfnpeer_v1_5_0()->radicle_simulation::node::Node"));
        assert!(code_no_space.contains("radicle_simulation::node::Node::new(\"peer-v1-5-0-0\")"));
        assert!(
            code_no_space.contains(
                "0=>radicle_simulation::node::Node::new(format!(\"peer-v1-5-0-{}\",index))"
            )
        );
    }

    #[test]
    fn test_generate_bindings_multi_replica() {
        let json = r#"{
            "peer-v1-7-0": {
                "replicas": 3
            }
        }"#;
        let code = generate_bindings(json).unwrap();
        let code_no_space = code.replace(" ", "");

        assert!(
            code_no_space.contains("pubfnpeer_v1_7_0(index:usize)->radicle_simulation::node::Node")
        );
        assert!(
            code_no_space
                .contains("radicle_simulation::node::Node::new(format!(\"peer-v1-7-0-{}\",index))")
        );
    }

    #[test]
    fn test_generate_bindings_relative_offsets() {
        let json = r#"{
            "peer-v1-7-0": { "replicas": 1 },
            "peer-v1-5-0": { "replicas": 1 },
            "peer-v1-6-1": { "replicas": 1 }
        }"#;
        let code = generate_bindings(json).unwrap();
        let code_no_space = code.replace(" ", "");

        assert!(code_no_space.contains(
            "-2=>radicle_simulation::node::Node::new(format!(\"peer-v1-5-0-{}\",index))"
        ));
        assert!(code_no_space.contains(
            "-1=>radicle_simulation::node::Node::new(format!(\"peer-v1-6-1-{}\",index))"
        ));
        assert!(
            code_no_space.contains(
                "0=>radicle_simulation::node::Node::new(format!(\"peer-v1-7-0-{}\",index))"
            )
        );

        assert!(code.contains("Available offsets: -2..=0"));
    }

    #[test]
    fn test_is_valid_cue_instance() {
        assert!(is_valid_cue_instance(StdPath::new("network.cue")));
        assert!(is_valid_cue_instance(StdPath::new("phase1_network.cue")));
        assert!(is_valid_cue_instance(StdPath::new(
            "/some/path/network.cue"
        )));

        assert!(!is_valid_cue_instance(StdPath::new("schema.cue")));
        assert!(!is_valid_cue_instance(StdPath::new(
            "../instances/schema.cue"
        )));

        assert!(!is_valid_cue_instance(StdPath::new("network.json")));
        assert!(!is_valid_cue_instance(StdPath::new("schema.json")));
        assert!(!is_valid_cue_instance(StdPath::new("README.md")));
    }
}
