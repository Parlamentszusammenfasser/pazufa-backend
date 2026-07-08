-- Add migration script here
INSERT INTO 
    stationstyp(value)
VALUES
    ('parl-antragsst'),
    ('preparl-formvs'),
    ('parl-vermittas'),
    ('parl-verfgstop')
;

INSERT INTO 
    dokumententyp (value)
VALUES
    ('gesetz'),
    ('eckpunktepapier')
;

