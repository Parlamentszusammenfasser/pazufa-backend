-- Changes for Spec 0.2.9 Database Migration

-- parlamentszusammenfasser#85
INSERT INTO dokumententyp(value)
VALUES
('berichtigung')
;

-- parlamentszusammenfasser#71
-- New track for SH, which handles Nachtragshaushalt differently to default
-- If possible, please all other BL also change to this track for 
-- Nachtragshaushalte, keeping the track as usual
INSERT INTO vorgangstyp(value)
VALUES
('gg-land-parl-staatsvertrag'),
('gg-land-parl-nachtragshaushalt')
;

DROP TABLE vgtyp_class;
