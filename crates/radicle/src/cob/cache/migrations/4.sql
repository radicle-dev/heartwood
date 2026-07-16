-- Materialize each issue's/patch's creation timestamp into an indexed column,
-- so listings can be ordered and paginated (keyset) without evaluating a JSON
-- subquery over every row on each query.

-- Creation timestamp of an issue: the first edit of its root comment (the one
-- without a `replyTo`), matching `Issue::timestamp`. Millisecond epoch.
alter table "issues" add column "timestamp" integer not null default 0;
update "issues" set "timestamp" = coalesce((
  select min(json_extract(comment.value, '$.edits[0].timestamp'))
  from json_each(json_extract(issue, '$.thread.comments')) as comment
  where json_extract(comment.value, '$.replyTo') is null
), 0);
create index if not exists 'ix_issues_repo_timestamp'
  on 'issues' (repo, timestamp desc, id desc);

-- Creation timestamp of a patch: the timestamp of its earliest revision, which
-- approximates `Patch::timestamp` (see `Patches::list_by_timestamp`).
alter table "patches" add column "timestamp" integer not null default 0;
update "patches" set "timestamp" = coalesce((
  select min(json_extract(revision.value, '$.timestamp'))
  from json_each(json_extract(patch, '$.revisions')) as revision
), 0);
create index if not exists 'ix_patches_repo_timestamp'
  on 'patches' (repo, timestamp desc, id desc);
