BEGIN;

create table if not exists FileTags (
  file text,
  tag text,
  primary key (file, tag)
) WITHOUT ROWID;

COMMIT;