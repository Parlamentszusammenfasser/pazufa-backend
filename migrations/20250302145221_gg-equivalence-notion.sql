-- Add migration script here


-- rename track_class into parl_class to make it more apparent what the idea is
-- and to set it apart from vgtyp_class, where equivalent vgs are maintained
CREATE TABLE parl_class (
    c_id INTEGER NOT NULL,
    p_id INTEGER NOT NULL UNIQUE REFERENCES parlament(id)
);

INSERT INTO parl_class (c_id, p_id) SELECT * FROM track_class;

DROP TABLE track_class;


-- which vg_typen can be used interchangably?
CREATE TABLE vgtyp_class (
    c_id INTEGER NOT NULL,
    t_id INTEGER NOT NULL UNIQUE REFERENCES vorgangstyp(id) 
);

INSERT INTO vgtyp_class

-- all others are already fine in their own track
SELECT id as i1, id as i2 
    FROM vorgangstyp v
    WHERE
        value <> 'gg-einspruch' AND
        value <> 'gg-zustimmung'

UNION

-- now, take the value for gg-einspruch and apply it to gg-zustimmung as well
SELECT v1.id, v2.id 
    FROM vorgangstyp v1, vorgangstyp v2
    WHERE
        v1.value = 'gg-einspruch' AND
        (v2.value = 'gg-zustimmung' OR v2.value = 'gg-einspruch')
;
