use radicle_simulation::setup_network;

// Sets up the network once for the whole suite.
// It generates `crate::network` and `crate::require_network`.
setup_network!("basic_cross_version_network");

mod cross_version_suite;
