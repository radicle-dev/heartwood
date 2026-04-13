// Shared constants for the Radicle simulation testing framework.
//
// These constants are used by both the main test framework (`src/`) and the
// build script (`build.rs`). Because `build.rs` is compiled separately, it
// includes this file directly via the `include!` macro.

/// The directory containing the CUE network topology instances.
pub const INSTANCES_DIR: &str = "../instances";

/// Schema filename
pub const SCHEMA_FILE: &str = "schema.cue";

/// The path to the shared CUE schema file.
pub const SCHEMA_CUE_PATH: &str = "../instances/schema.cue";

/// The CUE path used to extract the topology map during export.
pub const CUE_TOPOLOGY_PATH: &str = "values.topology";

/// The path to the Timoni module used to deploy the network.
#[allow(unused)]
pub const TIMONI_MODULE_PATH: &str = "../modules/radicle-node";

/// The name of the Timoni instance deployed to the Kubernetes cluster.
#[allow(unused)]
pub const TIMONI_INSTANCE_NAME: &str = "radicle-network";
