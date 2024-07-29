BEGIN;

create table if not exists FileTags (
  file text,
  tag text,
  primary key (file, tag)
) WITHOUT ROWID;

create table if not exists Config (
  id integer primary key,
  mountpoint text
) WITHOUT ROWID;

insert or ignore into Config(id) values (0);

COMMIT;