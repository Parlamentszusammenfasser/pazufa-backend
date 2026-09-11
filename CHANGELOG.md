# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

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
