--
-- Service configuration schema.
--

-- Node tracking policy.
create table if not exists "node-policies" (
  -- Node ID.
  "id"                 text      primary key not null,
  -- Node alias. May override the alias announced by the node.
  "alias"              text      default '',
  -- Tracking policy for this node.
  "policy"             text      default 'track'
  --
) strict;

-- Repository tracking policy.
create table if not exists "repo-policies" (
  -- Repository ID.
  "id"                 text      primary key not null,
  -- Tracking scope for this repository.
  --
  -- Valid values are:
  --
  -- "trusted"         track repository delegates and remotes in the `node-policies` table.
  -- "all"             track all remotes.
  --
  "scope"              text      default 'trusted',
  -- Tracking policy for this repository.
  "policy"             text      default 'track'
  --
) strict;
