-- Add the "relay" column.
-- The "relay" column can be set in several different ways:
--
-- * If set to `-1`, it means this announcement should *not* be relayed.
-- * If set to `NULL`, it means it *should* be relayed.
-- * If set to a positive integer, it means it has been relayed at that UNIX timestamp.
alter table "announcements" add column "relay" integer default -1;
