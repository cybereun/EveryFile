INSERT OR IGNORE INTO document_tags (document_id, tag_id)
SELECT document_tags.document_id,
       (
         SELECT canonical.id
         FROM tags AS canonical
         WHERE lower(canonical.name) = lower(tags.name)
         ORDER BY canonical.id
         LIMIT 1
       )
FROM document_tags
JOIN tags ON tags.id = document_tags.tag_id;

DELETE FROM document_tags
WHERE tag_id IN (
  SELECT duplicate.id
  FROM tags AS duplicate
  WHERE duplicate.id != (
    SELECT canonical.id
    FROM tags AS canonical
    WHERE lower(canonical.name) = lower(duplicate.name)
    ORDER BY canonical.id
    LIMIT 1
  )
);

DELETE FROM tags
WHERE id != (
  SELECT canonical.id
  FROM tags AS canonical
  WHERE lower(canonical.name) = lower(tags.name)
  ORDER BY canonical.id
  LIMIT 1
);

CREATE UNIQUE INDEX IF NOT EXISTS tags_name_nocase_unique
  ON tags(name COLLATE NOCASE);
