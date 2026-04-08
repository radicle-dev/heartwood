-- Add the version and user-agent columns.
alter table "nodes" add column "version" integer not null default 1;
alter table "nodes" add column "agent" text not null default "/radicle/";
-- Delete all cached announcements, since they no longer match our format.
delete from "announcements";
