use std::io;
use std::net;
use std::sync::LazyLock;

use schemars::{generate::*, *};

const SCHEMA_COMMAND: &str = "radicle::node::Command";
const SCHEMA_COMMAND_RESULT: &str = "radicle::node::CommandResult";
const SCHEMA_PROFILE_CONFIG: &str = "radicle::profile::Config";

const SCHEMAS: &[&str] = &[SCHEMA_COMMAND, SCHEMA_COMMAND_RESULT, SCHEMA_PROFILE_CONFIG];

pub static ERROR_MSG: LazyLock<String> = LazyLock::new(|| {
    let schemas = SCHEMAS.to_vec().join("\", \"");
    format!("Expected exactly one of the following schema names: [\"{schemas}\"].")
});

enum Schema {
    Command,
    CommandResult,
    ProfileConfig,
}

impl std::str::FromStr for Schema {
    type Err = io::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            SCHEMA_COMMAND => Ok(Self::Command),
            SCHEMA_COMMAND_RESULT => Ok(Self::CommandResult),
            SCHEMA_PROFILE_CONFIG => Ok(Self::ProfileConfig),
            schema => Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                format!("{schema}: {}", *ERROR_MSG),
            )),
        }
    }
}

impl Schema {
    fn from_args(mut args: std::env::Args) -> io::Result<Self> {
        let name = args.nth(1).ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidInput,
                format!("No schema given. {}", *ERROR_MSG),
            )
        })?;
        name.parse()
    }
}

fn main() {
    if let Err(e) = print_schema() {
        eprintln!("{}", e);
        std::process::exit(1);
    }
}

fn print_schema() -> io::Result<()> {
    let args = std::env::args();

    let name = Schema::from_args(args)?;

    let schema = match name {
        Schema::Command => {
            let settings = SchemaSettings::default().for_serialize();
            let generator = settings.into_generator();

            generator.into_root_schema_for::<radicle::node::Command>()
        }
        Schema::CommandResult => {
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
        Schema::ProfileConfig => schemars::schema_for!(radicle::profile::Config),
    };

    serde_json::to_writer_pretty(std::io::stdout(), &schema)?;
    Ok(())
}
