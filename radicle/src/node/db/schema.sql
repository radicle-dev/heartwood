-- Discovered nodes.
create table if not exists "nodes" (
  -- Node ID.
  "id"                 text      primary key not null,
  -- Node features.
  "features"           integer   not null,
  -- Node alias.
  "alias"              text      not null,
  --- Node announcement proof-of-work.
  "pow"                integer   default 0,
  -- Node announcement timestamp.
  "timestamp"          integer   not null,
  -- If this node is banned. Used as a boolean.
  "banned"             integer   default false
  --
) strict;

-- Node addresses. These are adresses advertized by a node.
create table if not exists "addresses" (
  -- Node ID.
  "node"               text      not null references "nodes" ("id") on delete cascade,
  -- Address type.
  "type"               text      not null,
  -- Address value, including port.
  "value"              text      not null,
  -- Where we got this address from.
  "source"             text      not null,
  -- When this address was announced.
  "timestamp"          integer   not null,
  -- Local time at which we last attempted to connect to this node.
  "last_attempt"       integer   default null,
  -- Local time at which we successfully connected to this node.
  "last_success"       integer   default null,
  -- If this address is banned from use. Used as a boolean.
  "banned"             integer   default false,
  -- Nb. This constraint allows more than one node to share the same address.
  -- This is useful in circumstances when a node wants to rotate its key, but
  -- remain reachable at the same address. The old entry will eventually be
  -- pruned.
  unique ("node", "type", "value")
  --
) strict;

-- Routing table. Tracks inventories.
create table if not exists "routing" (
  -- Repository being seeded.
  "repo"         text      not null,
  -- Node ID.
  "node"         text      not null references "nodes" ("id") on delete cascade,
  -- UNIX time at which this entry was added or refreshed.
  "timestamp"    integer   not null,

  primary key ("repo", "node")
);

-- Gossip message store.
create table if not exists "announcements" (
  -- Node ID.
  --
  -- Nb. We don't use a foreign key constraint here, because announcements are
  -- currently added to the database before nodes.
  "node"               text      not null,
  -- Repo ID, if any, for example in ref announcements.
  -- For other announcement types, this should be an empty string.
  "repo"               text      not null,
  -- Announcement type.
  --
  -- Valid values are:
  --
  -- "refs"
  -- "node"
  -- "inventory"
  "type"               text      not null,
  -- Announcement message in wire format (binary).
  "message"            blob      not null,
  -- Signature over message.
  "signature"          blob      not null,
  -- Announcement timestamp.
  "timestamp"          integer   not null,
  --
  unique ("node", "repo", "type")
  --
) strict;

-- Repository sync status.
create table if not exists "repo-sync-status" (
  -- Repository ID.
  "repo"                 text      not null,
  -- Node ID.
  "node"                 text      not null references "nodes" ("id") on delete cascade,
  -- Head of your `rad/sigrefs` branch that was synced.
  "head"                 text      not null,
  -- When this entry was last updated.
  "timestamp"            integer   not null,
  --
  unique ("repo", "node")
  --
) strict;
