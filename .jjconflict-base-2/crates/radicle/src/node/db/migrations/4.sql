-- Peer IP addresses.
create table if not exists "ips" (
  -- IP address. This *omits* the port.
  "ip"            text      primary key not null,
  -- Node this address belongs to. Can be `null` if the session is not yet
  -- established.
  "node"          text      references "nodes" ("id") on delete cascade,
  -- When this connection was last attempted by the peer.
  "last_attempt"  integer   not null,
  -- If this IP is banned. Used as a boolean.
  "banned"        integer   default false
  --
) strict;

