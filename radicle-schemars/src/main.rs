use std::io;
use std::net;

use schemars::{generate::*, *};

fn main() -> std::io::Result<()> {
    let mut args = std::env::args();

    let Some(name) = args.nth(1) else {
        return Err(io::Error::new(io::ErrorKind::InvalidInput, "Exactly one argument is required. It must be the fully qualified name of the schema requested, e.g. `radicle::node::Command`."));
    };

    let schema = match name.as_str() {
        "radicle::node::Command" => {
            let settings = SchemaSettings::default().for_serialize();
            let generator = settings.into_generator();

            generator.into_root_schema_for::<radicle::node::Command>()
        }
        "radicle::node::CommandResult" => {
            #[derive(JsonSchema)]
            #[allow(dead_code)]
            struct ListenAddrs(Vec<net::SocketAddr>);

            #[derive(JsonSchema)]
            #[allow(dead_code)]
            struct Error {
                error: String,
            }

            #[derive(JsonSchema)]
            #[schemars(untagged)]
            #[allow(dead_code)]
            enum CommandResult {
                Nid(
                    #[schemars(with = "radicle::schemars_ext::crypto::PublicKey")]
                    radicle::node::NodeId,
                ),
                Config(radicle::node::Config),
                ListenAddrs(ListenAddrs),
                ConnectResult(radicle::node::ConnectResult),
                Success(radicle::node::Success),
                Seeds(radicle::node::Seeds),
                FetchResult(radicle::node::FetchResult),
                RefsAt(radicle::storage::refs::RefsAt),
                Sessions(Vec<radicle::node::Session>),
                Session(Option<radicle::node::Session>),
                Error(Error),
            }
            schema_for!(CommandResult)
        }
        "radicle::profile::Config" => schemars::schema_for!(radicle::profile::Config),
        _ => {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Unknown schema requested.",
            ));
        }
    };

    serde_json::to_writer_pretty(std::io::stdout(), &schema)?;
    Ok(())
}
