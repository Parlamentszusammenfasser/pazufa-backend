-- Add migration script here
CREATE TABLE track_class(
    c_id INTEGER NOT NULL,
    p_id INTEGER NOT NULL
);

INSERT INTO track_class

SELECT id as i1, id as i2 from parlament p where value <> 'BT'
  and value <> 'BR'
  AND value <> 'EK'
  AND value <> 'BV'
UNION 

SELECT p1.id, p2.id from parlament p1, parlament p2 
WHERE p1.value = 'BT' AND 
(p2.value ='BR' OR p2.value = 'BT' OR p2.value = 'EK' OR p2.value = 'BV');


ALTER TABLE station ADD COLUMN exceptional BOOLEAN NOT NULL DEFAULT FALSE;
