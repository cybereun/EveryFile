ALTER TABLE documents ADD COLUMN parse_attempt_token TEXT;

CREATE UNIQUE INDEX documents_parse_attempt_token_unique
  ON documents(parse_attempt_token)
  WHERE parse_attempt_token IS NOT NULL;
