-- Git refs cache.
create table if not exists "refs" (
  -- Repository ID.
  "repo"                 text      not null,
  -- Ref namespace (NID).
  --
  -- Nb. We don't use a foreign key constraint because we can't guarantee
  -- that we'll have received a node announcement from this node.
  "namespace"            text      not null,
  -- Ref name (qualified).
  "ref"                  text      not null,
  -- Ref OID.
  "oid"                  text      not null,
  -- When this entry was created or updated.
  "timestamp"            integer   not null,
  --
  unique ("repo", "namespace", "ref")
  --
) strict;
