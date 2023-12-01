--
-- Node policy database.
--

-- Node follow policies.
create table if not exists "following" (
  -- Node ID.
  "id"                 text      primary key not null,
  -- Node alias. May override the alias announced by the node.
  "alias"              text      default '',
  -- Tracking policy for this node.
  "policy"             text      default 'allow'
  --
) strict;

-- Repository seeding policies.
create table if not exists "seeding" (
  -- Repository ID.
  "id"                 text      primary key not null,
  -- Tracking scope for this repository.
  --
  -- Valid values are:
  --
  -- "followed"        seed repository delegates and remotes in the `following` table.
  -- "all"             seed all remotes.
  --
  "scope"              text      default 'followed',
  -- Tracking policy for this repository.
  "policy"             text      default 'allow'
  --
) strict;
