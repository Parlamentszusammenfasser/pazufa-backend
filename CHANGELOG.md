# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).
## [0.3.5 / Spec 0.2.8] - 2026-10-23
### Added
- The deployed docker image now contains a copy of what the backend
  generates as openapi.yml
- There is a new Docker image in the pazufa/ repo, called
  pazufa/release-artifacts, which is versioned together with the
  backend. This is a `scratch` image containing 
  - arm64/amd64 binaries that are to be released
  - tracks.toml
  - openapi.yml
### Fixed
- The Version of the Spec is now checked in CI: If a branch starts with
  spec/*, it is considered for the spec workflow checking for new > old
- the spec is now deployed after build to a new branch in
  PaZuFa/parlamentszusammenfasser, and a PR is opened.
- dependency crates failing an audit were updated
### Changed
#### Internal Changes
- The set-version script now also checks if its Regexes match anything,
  and fails otherwise
### Impact on Hosting / Deployment
- this is practically the exact same version, just with a lot of new CI
  checks and the correct API version
### Impact on Scraper Maintainers
### Impact on API Consumers
- the version of the spec is now displayed correctly
## [0.3.4 / Spec 0.2.8] - 2026-10-14
### Added
- `if-modified-since` now also accepts RFC2822 Datetimes (#202)
- Doktyp has additional Variants (thanks @se-jaeger!) (#195)
  - beschlussprotokoll
  - inhaltsprotokoll
- Stationstyp has additional variants (#203)
  - `parl-anhoerung`
- Vorgangstyp has additional variants (#203)
  - `gg-land-haushalt-parl`
- Vorgang now features a vorgang-level summary-tuple vector
  (PazuFa/parlamentszusammenfasser#55)
- There is a naive Etag implementation that saves a bit of bandwidth (#113)
  This means there will be: 
  - strong `etag` headers on each object sent
  - a correct handling of `if-none-match`
  However, the implementation is not very efficient and still does a
  whole fetch / database thingy, only the bandwidth is saved.
- there is now a tiebreaker in place in the `Station` object that works
  by being a secondary order criterion after zp_start (#57)
- there is now a command line tool to trackvalidate vorgang objects
  given a tracks.toml. This simplifies checking if a debug dump is an
  actually sensible vorgang
### Changed
#### Internal Changes
- A few functions from pazufa-backend-lib/src/api/* were moved to the
  binary (pazufa-backend). These were mostly just thin wrappers around a
  single DB Query or similar. These are their opcodes:
  - `auth_status`
  - `auth_post`
  - `auth_delete`
  - `auth_metadata`
  - `keytag_listing`
  - `dokument_delete_by_id`
  - `sitzung_delete`
  - `kal_get`
  - `kal_date_get`
  - `s_get`
- renamed  LTZF* to Pazufa* (Server|Error)
- CreateAPIKey has been refactored
- Woodpecker rust docker testing image updated to rust 1.99; rust
  version used for building there and in the final dockerfile was added
  to `variables.toml`
- The insert module has been refactored
- Refactored validation logic, s.t. parliament classes (BR/BT being
  handled as one for example) are now defined in the binary 
#### External Changes
- Stations without any documents are now also matchable to stations in
  the database without documents. This should be watched.
### Deprecated
### Removed
- the `parl_class` table has been removed from the database (#209)
### Fixed
- Versioning Script had a false line regex to update the spec version (#194)
- Unicode characters producing `\0` in effect could crash transactions.
  These are now rejected. (#201)
- Regex Parsing could crash on parsing choiceboxes (thanks @nobkd!)
  (#208)
- The whole regex matching logic has been rewritten to use modified Glushkov-NFAs (#209)
  There is one breaking change for the track-validation crate which lead
  to a version bump: A regex which has leftover input after the regex
  has been consumed is now returning `false` instead of `true`; 
  This Fixes #207
- VGTypes other than `vgtyp` were silently dropped. This was due to the
  types not being inserted prior to inserting the tuples (#211)
### Security
### Impact on Scraper Maintainers
- Please use the freshly introduced `gg-parl-haushalt` for any 
  Haushaltsgesetzgebung in your BL
  For context see
  <https://codeberg.org/PaZuFa/pazufa-backend/issues/150>
- Please merge all Vorgänge that are descending from one
  Initiativdrucksache into a single vorgang, even Haushaltseinzelpläne
- Please merge Ausschusssitzungen together if they 
  - have the same gremium
  - happen in between the same reading
  - for more context and an example see <https://codeberg.org/PaZuFa/parlamentszusammenfasser/issues/53#issuecomment-23218126>
- Please DO NOT USE artificial `zp_start` values for ordering stations,
  instead use `meta_tiebreaker` to break ties
### Impact on API Consumers
- Please do note that the `etag` and `if-none-match` headers are now
  available to use
### Impact on Hosting / Deployment
- Please do note that the `etag` and `if-none-match` headers are now
  available to use for caching if available

## [0.3.3 / Spec 0.2.7] - 2026-09-12
### Added
- A Bulk delete Endpoint that is meant for deleting many `Vorgang` 
  objects at once by Filters. `DELETE /api/v2/vorgang` follows the same 
  semantics as `GET /api/v2/vorgang` with the exception of empty filters, 
  in which case calling this Endpoint is a No-Op (#190, Spec v0.2.7)
- `vorgang.schlagworte[]` was added and is meant to replace 
  `vorgang.stationen[].schlagworte[]`. Prefer this over the old ways.
  (#185, Spec v0.2.7)
- Nameable `Keytags`: `Keytags` can now be named by providing a field named
  `keytag_prefix` when calling `POST /api/v2/auth`. This field is
  limited to 10 Characters at a maximum and optional. The remaining 6
  characters will be filled with random alphanumeric characters.
- The Version of the spec is now also part of the struct returned by
  `/api/v2/status` (#184)

### Changed
#### Internal Changes
- Fixing #188 now means delete and a few others accept generic
  `PgExecutor` Objects
- `parl-verfgstopp` -> `postparl-vgstp` (#185, Spec v0.2.7)
- in CI an additional step now takes an old database dump, replays it
  and applies the new migrations to it. This is meant to catch a few
  nasty bugs we had in the past.

#### External Changes
- gg-einspruch and gg-zustimmung are replaced by 13 types specified
  here: (PaZuFa/parlamentszusammenfasser#62), a few of which are
  equivalent in three classes: those initiated by breg, those init'd by
  others and `bu-antrag-bweinsatz`

### Deprecated
- mirroring the addition above, `vorgang.stationen[].schlagworte[]` is
  being deprecated. It remains on the wire until a breaking change, when
  it will be removed together with `trojanergefahr`.
  All Schlagworte submitted in this field will henceforth be merged
  directly into `vorgang.schlagworte[]` by the backend.
### Removed
### Fixed
- The database is now no longer bootlooping when the backend database
  user is not SUPERUSER. The only requirement is `CREATEROLE`, with
  which a user named `pazufa_ro` is created, with the password supplied
  to the backend via environment variable, as before.
  Example: `ALTER USER pazufa_database_user CREATEROLE;`
- The Dockerfile now has a default value for `PAZUFA_PORT` baked
  in (8080). That should fix #173 (#175)
### Security
- New Passwords are now issued with Argon2id-Hashes; Existing passwords
  remain valid, but when all are expired that use sha256 this will be
  removed (#172)

### Impact on Scraper Maintainers
- Use `vorgang.schlagworte[]` instead of
  `vorgang.stationen[].schlagworte[]` to record keywords that have
  meaning for the whole vorgang. Otherwise use the keyword vector in
  `dokument`
- if you are trying to convey that a law was revoked by a constitutional court,
  the station type you are looking for is `postparl-vgstp`
- If you are maintaining the BT-Scraper, you should know about the
  `Vorgangstyp` entries that just arrived for you

### Impact on API Consumers
### Impact on Hosting / Deployment
- when provisioning keys, please tag them with something that can
  be recognized later, e.g.: BBKey, BYAdmin, MASTER, ...
- The bulk delete endpoint is for you, if you want to wipe staging.
  Example: `DELETE /api/v2/vorgang?p=BY` deletes all of Bayern.
