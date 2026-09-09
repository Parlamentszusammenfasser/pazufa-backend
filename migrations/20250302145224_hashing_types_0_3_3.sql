
CREATE TABLE hash_algorithm(
    id INTEGER PRIMARY KEY,
    value VARCHAR NOT NULL UNIQUE
);
INSERT INTO hash_algorithm(id, value)
VALUES
(1, 'sha256'),
(2, 'argon2id')
;

ALTER TABLE api_keys
    ADD COLUMN algo INTEGER NOT NULL REFERENCES hash_algorithm(id) DEFAULT 1 
;

ALTER TABLE api_keys ALTER COLUMN algo DROP DEFAULT;


-- making salt into a bytearray column
ALTER TABLE api_keys ALTER COLUMN salt DROP DEFAULT;

ALTER TABLE api_keys
    ALTER COLUMN salt 
    TYPE BYTEA
    USING convert_to(salt, 'UTF8');
