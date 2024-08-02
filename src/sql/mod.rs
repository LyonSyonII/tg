pub const FILE_EXISTS: &str = r#"--sql
    SELECT 1 FROM Files WHERE name = ?1 LIMIT 1
"#;

pub const TAG_EXISTS: &str = r#"--sql
    SELECT 1 FROM Tags WHERE tag = ?1 LIMIT 1
"#;

pub const INSERT_FILE: &str = r#"--sql
    INSERT OR IGNORE INTO Files (path) VALUES (?1)
"#;

pub const INSERT_TAG: &str = r#"--sql
    INSERT OR IGNORE INTO Tags (tag) VALUES (?1)
"#;

pub const INSERT_FILETAG: &str = r#"--sql
    INSERT OR IGNORE INTO FileTags (fileId, tagId)
        VALUES (
            (
                SELECT id
                  FROM Files
                 WHERE path = ?1
            ),
            (
                SELECT id
                  FROM Tags
                 WHERE tag = ?2
            )
        )
"#;

pub const GET_USED_TAGS: &str = r#"--sql
    SELECT DISTINCT t.tag
      FROM Tags t
      JOIN FileTags ft ON ft.tagId = t.id;
"#;

const MATCHING_TAGS_FILES: &str = r#"--sql 
-- Step 1: Define the target tags to match
WITH
    TargetTags AS ( VALUES {tags_list} ),

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
    DuplicateCheck AS MATERIALIZED (
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
"#;

pub fn matching_tags_files<D: std::fmt::Debug>(tags: impl IntoIterator<Item = D>) -> String {
    let tags_list = crate::utils::list_to_values(tags);
    format!(
        r#"--sql 
        -- Step 1: Define the target tags to match
        WITH
            TargetTags AS ( VALUES {tags_list} ),

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
            DuplicateCheck AS MATERIALIZED (
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
            FROM DuplicateCheck dc
    "#
    )
}

pub const MATCHING_TAGS_FILES_NO_DUPLICATEID: &str = r#"--sql
WITH
    TargetTags AS ( VALUES {tags_list} ),
    TagsLen AS (
        SELECT COUNT(*) AS len FROM TargetTags
    ),
    FoundFiles AS MATERIALIZED (
        SELECT ft.fileId as id
        FROM FileTags ft
        JOIN Tags t ON t.id = ft.tagId
        WHERE t.tag IN TargetTags
        GROUP BY ft.fileId
        HAVING COUNT(t.tag) = (SELECT len FROM TagsLen)
    ),
    FoundTags AS MATERIALIZED (
        SELECT DISTINCT t.tag
        FROM FileTags ft
        JOIN Tags t ON t.id = ft.tagId
        WHERE ft.fileId IN FoundFiles
        AND t.tag NOT IN TargetTags
    )
    SELECT t.tag, NULL AS file
    FROM FoundTags t
    UNION ALL
    SELECT NULL AS tag, f.file
    FROM FoundFiles ff
    JOIN Files f ON f.id = ff.id
"#;
