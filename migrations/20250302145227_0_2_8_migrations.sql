-- This Migration adds the required database changes for 0.2.8

-- parlamentszusammenfasser#55 zusammenfassungen on vorgang level
CREATE TABLE vg_zusammenfassung(
    vg_id INTEGER NOT NULL REFERENCES vorgang(id) ON DELETE CASCADE,
    zf_typ INTEGER NOT NULL REFERENCES zusammenf_typ(id) ON DELETE CASCADE,
    value VARCHAR NOT NULL,

    UNIQUE(vg_id, zf_typ)
);

-- #57 secondary ordering field
-- the field should be unique together with zp_start within any vorgang to make 
-- ordering the stations for track validation a guaranteed success
ALTER TABLE station
    ADD COLUMN
    tiebreak_ordering INTEGER DEFAULT NULL
;
ALTER TABLE station 
    ADD CONSTRAINT secondary_ordering_is_unique_with_zp_start 
        UNIQUE(vg_id, zp_start, tiebreak_ordering)
;

