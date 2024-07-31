BEGIN;

create table if not exists FileTags (
  file text,
  tag text,
  primary key (file, tag)
) WITHOUT ROWID;

create index if not exists FileTagsFileIdx on FileTags(file);
create index if not exists FileTagsTagIdx on FileTags(tag);

COMMIT;