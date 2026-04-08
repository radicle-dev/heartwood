-- Repository updates.
create table if not exists "repository-notifications" (
  -- Repository ID.
  "repo"               text      not null,
  -- Git reference name related to this update.
  "ref"                text      not null,
  -- Notification read status. Null if unread, otherwise the time it was read.
  "status"             integer   default null,
  -- Old head of the branch before update (OID or `null`).
  "old"                text,
  -- New head of the branch after update (OID or `null`).
  "new"                text,
  -- Update commit timestamp.
  "timestamp"          integer   not null,
  -- We only allow one notification per ref in a given repo. Newer
  -- notifications should replace older ones.
  unique ("repo", "ref")
) strict;

-- What updates are we subscribed to.
create table if not exists "repository-notification-interests" (
  -- Repository ID.
  "repo"               text      not null,
  -- Git reference glob to set interest on.
  -- To set interest on issues for eg., use "refs/cobs/xyz.radicle.issue/*"
  -- To set interest on all refs, use "refs/*"
  -- This can also be used to set interest on a specific COB or branch.
  "glob"               text      not null,
  -- Notification interest.
  --
  -- "all" - get all updates
  -- "none" - get no updates
  -- "relevant" - get updates if relevant to you
  "interest"           text      not null,
  --
  unique ("repo", "glob", "interest")
  --
) strict;
