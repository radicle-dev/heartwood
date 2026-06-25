# Build-time injection of support channel URLs

## Problem

The `check-keywords` pre-commit check forbids domain-specific strings
(`radicle.dev`, `radicle.xyz`, `radicle.zulipchat.com`) in `.rs` source files
to keep the client domain-agnostic. Two places in
`crates/radicle-cli/src/main.rs` hardcode support channel URLs and emails:

1. **`LONG_DESCRIPTION`** — the `--help` text with OSC 8 hyperlinks to the chat
   and feedback email.
2. **`human_panic::setup_panic!`** — the panic handler's support message with
   chat URL and support email.

## Design

### Approach

Inject domain-specific values at build time via environment variables, following
the existing pattern used for `GIT_HEAD`, `RADICLE_VERSION`, and
`SOURCE_DATE_EPOCH`.

### Environment variables

| Variable | Default | Used in |
|---|---|---|
| `RADICLE_CHAT_URL` | `https://radicle.zulipchat.com` | `LONG_DESCRIPTION`, `human_panic` |
| `RADICLE_FEEDBACK_EMAIL` | `feedback@radicle.dev` | `LONG_DESCRIPTION` |
| `RADICLE_SUPPORT_EMAIL` | `team@radicle.dev` | `human_panic` |

Values are full URLs / email addresses. Protocols (`mailto:`, etc.) are
prepended in `concat!()` where needed.

### Changes

#### 1. `build.rs` (workspace root)

Add three new env vars with defaults, following the existing pattern:

```rust
let chat_url = env::var("RADICLE_CHAT_URL")
    .unwrap_or_else(|_| "https://radicle.zulipchat.com".into());
let feedback_email = env::var("RADICLE_FEEDBACK_EMAIL")
    .unwrap_or_else(|_| "feedback@radicle.dev".into());
let support_email = env::var("RADICLE_SUPPORT_EMAIL")
    .unwrap_or_else(|_| "team@radicle.dev".into());

println!("cargo::rustc-env=RADICLE_CHAT_URL={chat_url}");
println!("cargo::rustc-env=RADICLE_FEEDBACK_EMAIL={feedback_email}");
println!("cargo::rustc-env=RADICLE_SUPPORT_EMAIL={support_email}");
```

All crate `build.rs` files are symlinks to the workspace root, so every crate
emits these vars. Only `radicle-cli` consumes them.

#### 2. `crates/radicle-cli/src/main.rs`

Replace hardcoded strings with `concat!` + `env!()`:

**`LONG_DESCRIPTION`:**
```rust
pub const LONG_DESCRIPTION: &str = concat!(
    "\nRadicle is a sovereign code forge built on Git.\n\n",
    "See `rad <COMMAND> --help` to learn about a specific command.\n\n",
    "Do you have feedback?\n",
    " - Chat <\x1b]8;;", env!("RADICLE_CHAT_URL"),
    "\x1b\\", env!("RADICLE_CHAT_URL"), "\x1b]8;;\x1b\\>\n",
    " - Mail <\x1b]8;;mailto:", env!("RADICLE_FEEDBACK_EMAIL"),
    "\x1b\\", env!("RADICLE_FEEDBACK_EMAIL"), "\x1b]8;;\x1b\\>\n",
    "   (Messages are automatically posted to the public #feedback channel on Zulip.)",
);
```

**`human_panic::setup_panic!`:**
```rust
human_panic::setup_panic!(human_panic::Metadata::new(
    env!("CARGO_PKG_NAME"),
    env!("CARGO_PKG_VERSION")
)
.homepage(env!("CARGO_PKG_HOMEPAGE"))
.support(concat!(
    "Open a support request at ", env!("RADICLE_CHAT_URL"),
    " or file an issue via Radicle itself, or e-mail to ",
    env!("RADICLE_SUPPORT_EMAIL")
)));
```

#### 3. `scripts/just/check-keywords.sh`

Filter out `build.rs` files from the arguments before running domain checks:

```bash
# Exclude build.rs files — they contain build-time defaults
# for domain-specific values that are injected via env vars.
ARGS=()
for arg in "$@"; do
    case "$arg" in
        */build.rs|build.rs) ;;
        *) ARGS+=("$arg") ;;
    esac
done
set -- "${ARGS[@]}"

if [ $# -eq 0 ]; then
    exit 0
fi
```

### Forks / overrides

Forks can override defaults without modifying source:

```sh
RADICLE_CHAT_URL=https://chat.example.com \
RADICLE_FEEDBACK_EMAIL=feedback@example.com \
RADICLE_SUPPORT_EMAIL=support@example.com \
cargo build
```

### Out of scope

- `radicle.xyz` mentions in `warning.rs` (seed node hostnames) and
  `fixtures.rs` (test email) are separate issues.
