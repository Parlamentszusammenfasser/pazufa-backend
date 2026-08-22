-- we are now in whatever database we want to be in by definition
CREATE USER pazufa_pg WITH PASSWORD 'pazufa_pg';
ALTER ROLE pazufa_pg CREATEROLE;

GRANT ALL PRIVILEGES ON DATABASE pazufa TO pazufa_pg;
GRANT ALL PRIVILEGES ON SCHEMA public TO pazufa_pg;
