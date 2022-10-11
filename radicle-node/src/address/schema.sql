--
-- Address book SQL schema.
--
create table if not exists "nodes" (
  -- Node ID.
  "id"                 text      primary key not null,
  -- Node features.
  "features"           integer   not null,
  -- Node alias.
  "alias"              text      default null,
  -- Node announcement timestamp.
  "timestamp"          integer   not null,
  -- Local time at which we last attempted to connect to this node.
  "last_attempt"       integer   default null,
  -- Local time at which we successfully connected to this node.
  "last_success"       integer   default null
  --
) strict;

create table if not exists "addresses" (
  -- Node ID.
  "node"               text      not null references "nodes" ("id"),
  -- Address type.
  "type"               text      not null,
  -- Address value.
  "value"              text      not null,
  -- Where we got this address from.
  "source"             text      not null,
  -- When this address was announced.
  "timestamp"          integer   not null,
  -- Nb. This constraint allows more than one node to share the same address.
  -- This is useful in circumstances when a node wants to rotate its key, but
  -- remain reachable at the same address. The old entry will eventually be
  -- pruned.
  unique ("node", "type", "value")
  --
) strict;
