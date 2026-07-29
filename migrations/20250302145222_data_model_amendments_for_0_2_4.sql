-- Add migration script here

CREATE TABLE mime(
    id SERIAL PRIMARY KEY,
    value VARCHAR NOT NULL UNIQUE
);
INSERT INTO mime(id, value)
VALUES
(1, 'application/json'),
(2, 'application/pdf'),
(3, 'text/plain'),
(4, 'text/html')
;

CREATE TABLE hash_strategy (
    id SERIAL PRIMARY KEY,
    value VARCHAR NOT NULL UNIQUE
);
INSERT INTO hash_strategy(id, value)
VALUES
(1, 'sha256+bytes'),
(2, 'sha256+text'),
(3, 'sha1+bytes')
;
-- Table of hashes associated with a document
CREATE TABLE dok_hashes(
    dok_id INTEGER NOT NULL REFERENCES dokument(id) ON DELETE CASCADE,
    hash_strat_id INTEGER NOT NULL REFERENCES hash_strategy(id) ON DELETE CASCADE,
    mime_id INTEGER NOT NULL REFERENCES mime(id) ON DELETE CASCADE,
    value VARCHAR NOT NULL,
    UNIQUE(dok_id, hash_strat_id)
);
INSERT INTO dok_hashes(dok_id, hash_strat_id, mime_id, value)
SELECT id, 1, 2, hash FROM dokument; -- migrate existing hashes: sha256+bytes & application/pdf per default


-- type of a summary, used in dokument at the moment
CREATE TABLE zusammenf_typ (
    id SERIAL PRIMARY KEY,
    value VARCHAR NOT NULL UNIQUE
);
INSERT INTO zusammenf_typ (id, value)
VALUES
(1, 'full'),        -- this value references the original zusammenfassung field, migrated here. Meaning: Just a "full" summary of the document without information about where it came from
(2, 'full-llm'),    -- summary is of the full document, and was made by AI
(3, 'full-extern')  -- summary is of the full document, and was taken from an external source
;

-- table containing the summaries attached to a document
-- a document can only contain one summary of any type
CREATE TABLE dok_zusammenfassungen(
    dok_id INTEGER NOT NULL REFERENCES dokument(id) ON DELETE CASCADE,
    zf_typ INTEGER NOT NULL REFERENCES zusammenf_typ(id) ON DELETE CASCADE,
    value VARCHAR NOT NULL,

    UNIQUE(dok_id, zf_typ)
);
-- Dokument: Migrate existing summaries into the new table, set `1`, meaning "full" as the type
INSERT INTO dok_zusammenfassungen (dok_id, zf_typ, value)

SELECT id, 1, zusammenfassung from dokument
WHERE zusammenfassung IS NOT NULL
;

-- remove the zusammenfassung column
ALTER TABLE dokument
    DROP COLUMN zusammenfassung;
-- remove the hash column
ALTER TABLE dokument
    DROP COLUMN hash;
-- Subdoc ID 
ALTER TABLE dokument
    ADD COLUMN subdoc_id INTEGER DEFAULT NULL
;

---- Vorgang Object changes
CREATE TABLE sachgebiet(
    id SERIAL PRIMARY KEY,
    value INTEGER NOT NULL UNIQUE,
    description VARCHAR NOT NULL
);
CREATE TABLE ressort(
    id SERIAL PRIMARY KEY,
    value VARCHAR NOT NULL UNIQUE
);
ALTER TABLE vorgang 
    ADD COLUMN ressort INTEGER REFERENCES ressort(id) DEFAULT NULL
;

CREATE TABLE vg_sachgebiete(
    vg_id INTEGER NOT NULL REFERENCES vorgang(id) ON DELETE CASCADE,
    sg_id INTEGER NOT NULL REFERENCES sachgebiet(id) ON DELETE CASCADE,

    PRIMARY KEY(vg_id, sg_id)
);


INSERT INTO sachgebiet(value, description) 
VALUES 

(1000,'Staat und Politik'),
(1010,'Staatsaufbau'),
(1030,'Menschenrechte'),
(1050,'Nation'),
(1060,'Ideologien'),
(1070,'Politische Kräfte'),
(1080,'Wahlen'),
(1100,'Parlament'),
(1110,'Abgeordnete'),
(1200,'Öffentliche Verwaltung'),
(1210,'Bundesregierung'),
(1220,'Landesregierung'),
(1230,'Kommunale Angelegenheiten'),
(1240,'Öffentlicher Dienst'),
(1300,'Innere Sicherheit'),
(1310,'Polizei'),
(1320,'Verfassungsschutz, Spionage'),
(1330,'Ordnungsrecht'),
(1340,'Katastrophen- und Zivilschutz'),
(1350,'Rettungswesen'),
(1400,'Verteidigung'),
(1410,'Wehrdienst'),
(1420,'Rüstung'),
(1430,'Abrüstung, Konversion'),
(1500,'Außenpolitik'),
(1510,'Internationale Beziehungen'),
(1520,'Internationale Organisationen'),
(1530,'Entwicklungszusammenarbeit'),
(1540,'Europapolitik'),
(1600,'Europäische Union'),
(1610,'Organe der EU'),
(1620,'Programme der EU'),
(2000,'Wirtschaft'),
(2010,'Gewerbliche Wirtschaft, Industrie'),
(2020,'Handel'),
(2030,'Dienstleistungen'),
(2040,'Versicherungen'),
(2050,'Mittelständische Wirtschaft'),
(2060,'Außenwirtschaft'),
(2070,'Verbraucher'),
(2080,'Preis- und Kartellrecht'),
(2090,'Gewerbeaufsicht'),
(2100,'Energie'),
(2110,'Fossile Energien'),
(2120,'Kernenergie'),
(2130,'Erneuerbare Energien'),
(2200,'Bergbau, Bodenschätze'),
(2300,'Technologie'),
(2400,'Arbeit und Beschäftigung'),
(2410,'Arbeitsmarkt'),
(2420,'Berufsausbildung'),
(2430,'Arbeitsentgelt'),
(2440,'Mitbestimmung'),
(2450,'Arbeitsbedingungen'),
(2500,'Standardisierung'),
(2510,'Normung'),
(2520,'Eich- und Messwesen'),
(2530,'Vermessungs- und Katasterwesen'),
(2600,'Verkehr'),
(2610,'Öffentlicher Personenverkehr'),
(2620,'Güterverkehr'),
(2630,'Straßenverkehr'),
(2640,'Schienenverkehr'),
(2650,'Luftverkehr'),
(2660,'Schifffahrt'),
(2700,'Raumfahrt'),
(2800,'Bauwesen'),
(2810,'Verkehrswegebau'),
(2820,'Städtebau'),
(2830,'Wohnungswesen'),
(2840,'Wasserbau'),
(3100,'Recht'),
(3110,'Strafrecht'),
(3120,'Zivilrecht'),
(3130,'Öffentliches Recht'),
(3140,'Urheberschutz'),
(3200,'Verfassungsgerichtsbarkeit'),
(3300,'Justiz'),
(3310,'Gerichte und Staatsanwaltschaften'),
(3320,'Justizverwaltung'),
(3330,'Justizvollzug'),
(3400,'Juristische Berufe'),
(4100,'Bildung'),
(4200,'Schulen'),
(4210,'Lehrer'),
(4220,'Allgemeinbildende Schulen'),
(4230,'Berufsbildende Schulen'),
(4240,'Sonderpädagogik'),
(4250,'Privatschulen'),
(4260,'Frühkindliche Bildung'),
(4300,'Hochschulwesen'),
(4310,'Universitäten'),
(4320,'Kunst- und Musikhochschulen'),
(4330,'Hochschulen für angewandte Wissenschaften'),
(4400,'Wissenschaft, Forschung'),
(4500,'Erwachsenenbildung'),
(5000,'Gesellschaft, Bevölkerung'),
(5010,'Lebensgemeinschaften'),
(5020,'Sexuelle Identität'),
(5030,'Kinder, Jugendliche'),
(5040,'Frauen, Männer'),
(5050,'Menschen mit Behinderungen'),
(5060,'Alte Menschen'),
(5070,'Ausländer, Migranten'),
(5080,'Sonstige gesellschaftliche Gruppen'),
(5100,'Soziales'),
(5110,'Sozialversicherung'),
(5120,'Sozialleistungen'),
(5130,'Soziale Einrichtungen'),
(5140,'Versorgung'),
(5150,'Pflege'),
(5200,'Gesundheit'),
(5210,'Gesundheitsschutz'),
(5220,'Gesundheitseinrichtungen'),
(5230,'Medizinische Berufe'),
(5240,'Arzneimittel'),
(5250,'Rauschmittel'),
(5260,'Psychiatrie'),
(5270,'Tod'),
(6100,'Umwelt'),
(6110,'Natur'),
(6120,'Tier, Tierschutz, Tierhaltung'),
(6130,'Boden'),
(6140,'Wasser'),
(6150,'Klima'),
(6160,'Schadstoffe, Immissionen und Emissionen'),
(6200,'Abfall'),
(6300,'Abwasser'),
(6400,'Raumordnung'),
(6410,'Ländlicher Raum'),
(6500,'Landwirtschaft'),
(6510,'Landwirtschaftliche Betriebe'),
(6520,'Agrarmarkt'),
(6530,'Landwirtschaftliche Berufe'),
(6600,'Wald, Forsten'),
(6700,'Jagd, Fischerei'),
(6800,'Ernährung'),
(6900,'Tierkrankheiten'),
(7100,'Kunst, Kultur'),
(7200,'Denkmalschutz, Denkmalpflege'),
(7300,'Religionsgemeinschaften'),
(7400,'Freizeit'),
(7500,'Messen, Ausstellungen'),
(7600,'Sport'),
(7700,'Informationsgesellschaft, Medien'),
(7710,'Printmedien'),
(7720,'Rundfunk, Fernsehen'),
(7730,'Film, Video'),
(7740,'Informations- und Kommunikationstechnologien'),
(7750,'Datenschutz'),
(7800,'Statistik'),
(8100,'Abgaben'),
(8200,'Finanzverwaltung'),
(8300,'Öffentlicher Haushalt'),
(8310,'Öffentliche Schulden'),
(8320,'Öffentliches Vermögen'),
(8330,'Haushaltskontrolle'),
(8340,'Finanzausgleich'),
(8350,'Öffentliche Vergabe'),
(8400,'Finanzmarkt'),
(8600,'Vermögen'),
(8700,'Glücksspiel'),
(8800,'Stiftung'),
(9900,'Unbekannt'),
(9999,'ohne@-Systematik')
;

INSERT INTO ressort(value)
VALUES

('Arbeit'),
('Bildung'),
('Digitalisierung'),
('Energie'),
('Ernährung'),
('Europa'),
('Familie/Senioren'),
('Finanzen'),
('Forschung'),
('Forsten'),
('Frauen/Gleichstellung'),
('Gesundheit/Pflege/Prävention'),
('Heimat'),
('Inneres'),
('Integration/Migration'),
('Jugend'),
('Justiz'),
('Kinder'),
('Klimaschutz'),
('Kommunales'),
('Kunst/Kultur'),
('Landes-/Stadtentwicklung'),
('Ländlicher Raum'),
('Landwirtschaft'),
('Soziales'),
('Sport'),
('Tourismus'),
('Umwelt'),
('Verbraucherschutz'),
('Verkehr/Infrastruktur'),
('Wirtschaft'),
('Wissenschaft'),
('Wohnen/Bau')
;

