PRAGMA case_sensitive_like = true;

-- TABLES
CREATE TABLE IF NOT EXISTS Files (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  path TEXT NOT NULL UNIQUE,
  file TEXT
);

CREATE TABLE IF NOT EXISTS Tags (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  tag TEXT NOT NULL UNIQUE
);

CREATE TABLE IF NOT EXISTS FileTags (
  fileId INTEGER,
  tagId INTEGER,
  duplicateId INTEGER NOT NULL DEFAULT 0,
  PRIMARY KEY (fileId, tagId),
  FOREIGN KEY (fileId) REFERENCES Files(id),
  FOREIGN KEY (tagId) REFERENCES Tags(id)
) WITHOUT ROWID;

-- TRIGGERS
DROP TRIGGER IF EXISTS FilesSetName;
CREATE TRIGGER IF NOT EXISTS FilesSetName
AFTER INSERT ON Files
FOR EACH ROW
BEGIN
    UPDATE Files SET 
    file = (SELECT replace(path, rtrim(path, replace(path, '/', '')), ''))
	WHERE id = NEW.id;
END;

DROP TRIGGER IF EXISTS FileTagsSetDuplicateId;
CREATE TRIGGER IF NOT EXISTS FileTagsSetDuplicateId
AFTER INSERT ON FileTags
FOR EACH ROW
BEGIN
    UPDATE FileTags
	SET duplicateId = (
        SELECT COUNT(*)
        FROM FileTags ft2
        JOIN Files f1 ON NEW.fileId = f1.id
        JOIN Files f2 ON ft2.fileId = f2.id
        WHERE ft2.tagId = NEW.tagId AND NEW.fileId != ft2.fileId AND f1.file = f2.file
    )
	WHERE fileId = NEW.fileId and tagId = NEW.tagId;
END;

-- INDICES
CREATE INDEX IF NOT EXISTS FilesFileIdx ON Files(file);
CREATE INDEX IF NOT EXISTS TagsTagIdx ON Tags(tag);

CREATE INDEX IF NOT EXISTS FileTagsFileIdx ON FileTags(fileId);
CREATE INDEX IF NOT EXISTS FileTagsTagIdx ON FileTags(tagId);
CREATE INDEX IF NOT EXISTS FileTagsTagIdx ON FileTags(fileId, tagId);
CREATE INDEX IF NOT EXISTS FileTagsTagFileIdx ON FileTags(tagId, fileId);