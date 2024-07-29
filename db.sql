create table if not exists FileTags (
  file text,
  tag text,
  
  primary key (file, tag)
) WITHOUT ROWID;
  
insert into FileTags values
  ("/show must go on.mp3", "rock"),
  ("/show must go on.mp3", "mp3"),
  ("/show must go on.mp3", "music"),
  ("/show must go on.mp3", "queen"),
  
  ("/boig per tu.mp4", "rock"),
  ("/boig per tu.mp4", "music"),
  
  ("/bethoven.mp3", "music"),
  ("/bethoven.mp3", "mp3"),
  ("/bethoven.mp3", "classic"),
  
  ("/mozart.flac", "music"),
  ("/mozart.flac", "classic"),
  ("/mozart.flac", "flac");

-- /queen => ["rock", "music", "mp3"]
-- /mp3 => ["rock", "music", "queen"]
-- /queen/mp3 => ["music", "rock"]
-- /classic => ["music", "mp3", "flac"]
-- /classic/flac => ["music"]

-- donada una o mes tags, troba les tags dels arxius amb aquestes tags

select tag from FileTags where file in (
  select file from FileTags
  where tag in ("queen")
  group by file 
  having count(*) = 1
) and tag not in ("queen");