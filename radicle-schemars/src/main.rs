use std::io;
use std::net;

use schemars::{generate::*, *};

const SCHEMA_COMMAND: &str = "radicle::node::Command";
const SCHEMA_COMMAND_RESULT: &str = "radicle::node::CommandResult";
const SCHEMA_PROFILE_CONFIG: &str = "radicle::profile::Config";

const SCHEMAS: &[&str] = &[SCHEMA_COMMAND, SCHEMA_COMMAND_RESULT, SCHEMA_PROFILE_CONFIG];

#[inline]
fn unknown_schema(schema: Option<String>) -> io::Result<()> {
    let schema = match schema {
        Some(schema) => format!("Unexpected schema name \"{schema}\" given."),
        None => "No schema name given.".into(),
    };
    let schemas = SCHEMAS.to_vec().join("\", \"");
    Err(io::Error::new(
        io::ErrorKind::InvalidInput,
        format!("{schema} Expected exactly one of the following schema names: [\"{schemas}\"]."),
    ))
}

fn main() {
    if let Err(e) = print_schema() {
        eprintln!("{}", e);
        std::process::exit(1);
    }
}

fn print_schema() -> io::Result<()> {
    let mut args = std::env::args();

    let Some(name) = args.nth(1) else {
        return unknown_schema(None);
    };

    if !SCHEMAS.contains(&&name.as_str()) {
        return unknown_schema(Some(name));
    }

    let schema = match name.as_str() {
        SCHEMA_COMMAND => {
            let settings = SchemaSettings::default().for_serialize();
            let generator = settings.into_generator();

            generator.into_root_schema_for::<radicle::node::Command>()
        }
        SCHEMA_COMMAND_RESULT => {
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
        SCHEMA_PROFILE_CONFIG => schemars::schema_for!(radicle::profile::Config),
        _ => {
            return unknown_schema(Some(name));
        }
    };

    serde_json::to_writer_pretty(std::io::stdout(), &schema)?;
    Ok(())
}
