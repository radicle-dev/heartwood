-- `get_inventory` looks routing entries up by node alone, once per inbound
-- inventory announcement. The table's only index is the implicit one for the
-- primary key `(repo, node)`, whose leading column is `repo`, so that lookup
-- scans -- tens of thousands of rows on a seed following a busy network.
create index if not exists "routing_node" on "routing" ("node");
