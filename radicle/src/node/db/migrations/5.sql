-- Add the user-agent column.
alter table "nodes" add column "agent" text not null default "/radicle/";
