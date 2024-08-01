-- # All the currently used tags
SELECT DISTINCT t.tag
FROM FileTags ft
JOIN Tags t ON t.id = ft.tagId;


-- # Get both files and tags that match the specified list
-- # Results are as follows:
-- # { 
-- #   tag: text | null,
-- #   file: text | null
-- # }[]
WITH
-- Tag list
TargetTags AS ( VALUES ('tag1') ),

-- Identify files that have exactly the target tags
FoundFiles AS MATERIALIZED (
    SELECT ft.fileId as id, ft.duplicateId as duplicateId
    FROM FileTags ft
    JOIN Tags t ON t.id = ft.tagId
    WHERE t.tag IN TargetTags
    GROUP BY ft.fileId
    HAVING COUNT(t.tag) = (SELECT COUNT(*) FROM TargetTags)
),

-- Main query to find tags for these files, excluding specific tags
FoundTags AS MATERIALIZED (
    SELECT DISTINCT t.tag
    FROM FileTags ft
    JOIN Tags t ON t.id = ft.tagId
    WHERE ft.fileId IN (select id from FoundFiles)
    AND t.tag NOT IN TargetTags
)

-- Select results combining FoundTags and FoundFiles
SELECT t.tag, NULL AS file
FROM FoundTags t
UNION ALL           -- TODO: Optimize this aberration
SELECT NULL AS tag, iif( exists(select duplicateId from FoundFiles where file = f.file AND duplicateId > 0), ff.duplicateId || "_", "") || f.file
FROM FoundFiles ff
JOIN Files f ON f.id = ff.id;