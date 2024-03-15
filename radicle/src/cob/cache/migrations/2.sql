-- Identities
create table if not exists "identities" (
  -- Identity ID
  "id"            text      not null,
  -- Repository ID
  "repo"          text      not null,
  -- Identity in JSON format
  "identity"         text      not null,
  -- N.b. There must only be a single Identity for each Repository.
  unique ("id", "repo")
) strict;
