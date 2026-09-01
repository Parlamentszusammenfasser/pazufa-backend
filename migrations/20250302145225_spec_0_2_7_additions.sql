-- containing additions made through: 
-- PaZuFa/parlamentszusammenfasser#54 (Schlagworte on vorgang level)


CREATE TABLE rel_vorgang_schlagwort(
    vg_id INTEGER NOT NULL REFERENCES vorgang(id) ON DELETE CASCADE,
    sw_id INTEGER NOT NULL REFERENCES schlagwort(id)ON DELETE CASCADE,
    PRIMARY KEY (vg_id, sw_id)
);

-- PaZuFa/parlamentszusammenfasser#62 (bt-values)

INSERT INTO vorgangstyp(value)
VALUES
-- first class: initiated by breg
('bu-einspruch-inibreg'),
('bu-einspruch-inibreg-haushalt'),
('bu-zustimmung-inibreg'),
('bu-zustimmung-inibreg-haushalt'),
('bu-einspruch-inibreg-intvertrag'),
('bu-zustimmung-inibreg-intvertrag'),

-- second class: initiated by bt / br
('bu-einspruch-inisonst'),
('bu-einspruch-inisonst-intvertrag'),
('bu-zustimmung-inisonst'),
('bu-zustimmung-inisonst-intvertrag'),

('bu-antrag-bweinsatz')
;

-- vgtyp_class has no ON DELETE CASCADE; need to redo the constraint
ALTER TABLE vgtyp_class 
    DROP CONSTRAINT vgtyp_class_t_id_fkey;

ALTER TABLE vgtyp_class 
    ADD CONSTRAINT vgtyp_class_t_id_fkey
    FOREIGN KEY (t_id) 
    REFERENCES vorgangstyp(id)
    ON DELETE CASCADE;

-- new vgtyp equivalences
INSERT INTO vgtyp_class(c_id, t_id)
SELECT v1.id, v2.id 
FROM vorgangstyp v1, vorgangstyp v2
WHERE 
-- first class: initiated by regierung
v1.value = 'bu-einspruch-inibreg'             AND (
    v2.value = v1.value OR 
    v2.value = 'bu-zustimmung-inibreg' OR
    v2.value = 'bu-zustimmung-inibreg-intvertrag' OR 
    v2.value = 'bu-einspruch-inibreg-intvertrag' OR 
    v2.value = 'bu-zustimmung-inibreg-haushalt' OR
    v2.value = 'bu-einspruch-inibreg-haushalt'
) 
OR
-- second class: initiated by bt or br
v1.value = 'bu-einspruch-inisonst'            AND (
    v2.value = v1.value OR 
    v2.value = 'bu-zustimmung-inisonst' OR
    v2.value = 'bu-einspruch-inisonst-intvertrag' OR 
    v2.value = 'bu-zustimmung-inisonst-intvertrag'
) 
;
-- third class: antrag / bundeswehreinsatz
INSERT INTO vgtyp_class(c_id, t_id)
SELECT id, id FROM vorgangstyp
WHERE value = 'bu-antrag-bweinsatz';


-- move all vorgang objects over to new types
UPDATE vorgang SET
        typ = (SELECT id FROM vorgangstyp WHERE value = 'bu-zustimmung-inibreg-haushalt' LIMIT 1)
WHERE   typ = (SELECT id from vorgangstyp WHERE value = 'gg-zustimmung');

UPDATE vorgang SET
        typ = (SELECT id FROM vorgangstyp WHERE value = 'bu-einspruch-inibreg-haushalt' LIMIT 1)
WHERE   typ = (SELECT id from vorgangstyp WHERE value = 'gg-einspruch');

UPDATE vorgang SET
        typ = (SELECT id FROM vorgangstyp WHERE value = 'bu-antrag-bweinsatz' LIMIT 1)
WHERE   typ = (SELECT id from vorgangstyp WHERE value = 'bw-einsatz');

DELETE FROM vorgangstyp WHERE
value = 'gg-einspruch' OR
value = 'gg-zustimmung' OR
value = 'bw-einsatz'
;


-- PaZuFa/parlamentszusammenfasser#73 (parl-verfgstopp renamed)
UPDATE stationstyp
SET     value = 'postparl-vgstp'
WHERE   value = 'parl-verfgstop';
