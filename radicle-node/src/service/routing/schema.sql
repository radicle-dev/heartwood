--
-- Routing table SQL schema.
--
create table if not exists "routing" (
  "id"           integer   primary key,
  -- Resource being seeded.
  "resource"     text      not null,
  -- Node ID.
  "node"         text      not null,
  -- UNIX time at which this entry was added or refreshed.
  "time"         integer   not null,

  unique("resource", "node")
);
create index "routing_index" on routing ("resource", "node");
