# Database compatibility

Orca upgrades old libraries on startup. A successful migration must preserve
playlists, settings, and metadata, and it must be transactional. Test upgrades
with disposable copies of older databases; never test against a user's library.

On Windows, 0.1.8 uses `%LOCALAPPDATA%\Orca` as its stable profile directory.
It moves an existing 0.1.7 native profile from
`%LOCALAPPDATA%\OrcaSlintTauri` on first launch, then updates stored local
artwork and settings paths. A direct upgrade from the 0.1.6 Tauri app still
imports `%APPDATA%\orca\orca.db` when no native library exists. If both native
profile directories contain libraries before migration, the 0.1.7 native
library takes priority and the existing new folder is retained as an automatic
backup. Once the new profile is marked migrated, subsequent releases reuse it.

`PRAGMA user_version` is the minimum schema version a reader must support. The
released native app supports version 2. Compatible additions, such as the
`liked_songs` table, retain version 2 because that app can safely ignore them.
Each addition is recorded in `orca_schema_migrations` so it runs once. A later
change that removes or changes data used by a version 2 reader must raise
`user_version` and make a consistent SQLite backup before migrating. Older
builds must refuse to open that newer database; rollback requires restoring
the matching backup, not editing the version number.

Before release, test both directions with disposable profiles: upgrade a copy
of the previous release's library and check that user data survives; for a
compatible addition, launch the previous release against the upgraded copy.
For a breaking migration, verify the previous release refuses the upgraded
copy and can open the backup after restoration.

Version 3 was briefly used by unreleased Liked Songs builds for an additive
table. It is reserved for that preview schema and must not be reused for a
different migration. Current builds recognize its Liked Songs table and repair
its reader version to 2 without deleting the table or likes. Unknown newer
versions are rejected before WAL mode or migrations can modify the database.

For development, pass `--data-dir` with a disposable profile. A development
build using the default profile shares it with the installed release and can
upgrade that release's database.
