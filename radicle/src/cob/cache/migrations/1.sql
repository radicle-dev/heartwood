-- Issues
create table if not exists "issues" (
       -- Issue ID
       "id"            text      primary key not null,
       -- Repository ID
       "repo"          text      not null,
       -- Issue in JSON format
       "issue"         text      not null
) strict;

-- Patches
create table if not exists "patches" (
       -- Patch ID
       "id"            text      primary key not null,
       -- Repository ID
       "repo"          text      not null,
       -- Patch in JSON format
       "patch"         text      not null
) strict;
