-- # All the currently used tags
   SELECT DISTINCT t.tag
     FROM FileTags ft
     JOIN Tags t ON t.id = ft.tagId;


-- Step 1: Define the target tags to match
WITH
    TargetTags AS ( VALUES ("config") ),

-- Step 2: Calculate the number of target tags
    TagsLen AS (
        SELECT COUNT(*) AS len FROM TargetTags
    ),

-- Step 3: Find files that match all target tags
-- Include file ID and duplicate ID for further processing
    FoundFiles AS MATERIALIZED (
        SELECT ft.fileId as id, ft.duplicateId as duplicateId
        FROM FileTags ft
        JOIN Tags t ON t.id = ft.tagId
        WHERE t.tag IN TargetTags
        GROUP BY ft.fileId
        HAVING COUNT(t.tag) = (SELECT len FROM TagsLen)
    ),

-- Step 4: Find additional tags associated with found files
-- that are not in the target tags
    FoundTags AS MATERIALIZED (
        SELECT DISTINCT t.tag
        FROM FileTags ft
        JOIN Tags t ON t.id = ft.tagId
        WHERE ft.fileId IN (SELECT id FROM FoundFiles)
        AND t.tag NOT IN TargetTags
    ),

-- Step 5: Check for duplicates and adjust file names if necessary
    DuplicateCheck AS (
        SELECT f.id,
               f.name,
               CASE
                   -- Check if there's another file with the same name
                   -- and a non-zero duplicate ID
                   WHEN EXISTS (
                       SELECT 1
                       FROM Files f2
                       JOIN FoundFiles ff2 ON f2.id = ff2.id
                       WHERE f2.name = f.name
                         AND f2.id != f.id
                         AND ff2.duplicateId > 0
                   )
                   -- Append duplicate ID to the name if a duplicate exists
                   THEN ff.duplicateId || '_' || f.name
                   -- Keep the original name if no duplicate exists
                   ELSE f.name
               END AS adjusted_name
        FROM Files f
        JOIN FoundFiles ff ON f.id = ff.id
    )

-- Step 6: Combine and return the results
-- Return found tags
SELECT t.tag, NULL AS file
FROM FoundTags t
UNION ALL
-- Return found files with adjusted names
SELECT NULL AS tag, dc.adjusted_name AS file
FROM DuplicateCheck dc;
























































































































-- # Get both files and tags that match the specified list
-- # Results are as follows:
-- # { 
-- #   tag: text | null
, -- #   name: text | null
-- # }[]
     WITH
          -- Tag list
          TargetTags AS (
             VALUES ('tag1')
          ),
          -- Identify files that have exactly the target tags
          FoundFiles AS MATERIALIZED (
             SELECT ft.fileId AS id,
                    ft.duplicateId AS duplicateId
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
              WHERE ft.fileId IN (
                       SELECT id
                         FROM FoundFiles
                    )
                AND t.tag NOT IN TargetTags
          )
          -- Select results combining FoundTags and FoundFiles
   SELECT t.tag,
          NULL AS name
     FROM FoundTags t
UNION ALL
   SELECT NULL AS tag,
          -- Check if a duplicate of the selected file exists, and prepend the duplicateId
          CASE WHEN ff.duplicateId > 0
               OR EXISTS (
                    SELECT 1
                    FROM FoundFiles ff2
                    WHERE ff2.id != f.id
                         AND ff2.name = f.name
                         AND ff2.duplicateId > 0
               ) THEN ff.duplicateId || "_"
               -- No duplicate exists, do not prepend
               ELSE ""
          END || f.name AS name
     FROM FoundFiles ff
     JOIN Files f ON f.id = ff.id;