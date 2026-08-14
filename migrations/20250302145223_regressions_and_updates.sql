-- fixes #158.1
SELECT setval('zusammenf_typ_id_seq',   (SELECT MAX(id) FROM zusammenf_typ));
SELECT setval('hash_strategy_id_seq',   (SELECT MAX(id) FROM hash_strategy));
SELECT setval('mime_id_seq',            (SELECT MAX(id) FROM mime));


-- adds metadata to tables.
-- the last_mod dates refer to the data objects, rather than the underlying
-- vorgang/dok/sitzung in the parliament. 
-- this is a better guard for the elasticsearch companion and other if-mod-since
-- queries on the database and only require a rather minor amount of bookkeeping
ALTER TABLE vorgang
    ADD COLUMN 
    meta_last_modified TIMESTAMPTZ NOT NULL DEFAULT NOW()
;

ALTER TABLE dokument
    ADD COLUMN
    meta_last_modified TIMESTAMPTZ NOT NULL DEFAULT NOW()
;

ALTER TABLE sitzung
    ADD COLUMN
    meta_last_modified TIMESTAMPTZ NOT NULL DEFAULT NOW()
;

-- populate vorgang meta
UPDATE vorgang v
SET 
meta_last_modified = (
    SELECT COALESCE(MAX(s.zp_start), NOW()) FROM station s WHERE s.vg_id = v.id
);


-- populate dokument meta
WITH dokument_with_max_tstamp AS (
SELECT id, zp_lastmod as tstamp FROM dokument

UNION ALL 
SELECT id, zp_referenz as tstamp FROM dokument

UNION ALL 
SELECT id, zp_created as tstamp FROM dokument
WHERE zp_created IS NOT NULL
)

UPDATE dokument d
SET 
meta_last_modified = (SELECT COALESCE(MAX(tstamp)) FROM dokument_with_max_tstamp dt WHERE dt.id = d.id);


-- populate sitzung meta
UPDATE sitzung
SET 
meta_last_modified = last_update;

ALTER TABLE sitzung DROP last_update;


