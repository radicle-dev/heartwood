-- Add the "penalty" column.
-- Higher numbers reduce the chances that we connect to this node.
alter table "nodes" add column "penalty" integer not null default 0;
