# Dosty identity and compatibility

The application is **Dosty**, with lowercase `dosty` in its outlined wordmark.
The Cargo package and executable are `dosty`; the macOS bundle is
`target/Dosty.app` with identifier `app.dosty.editor`. The source repository is
[MoonlarkStudios/dosty](https://github.com/MoonlarkStudios/dosty).

## Existing Koan and Daytrip data

The Dosty rename needs no data migration. Preferences deliberately continue using
`ProjectDirs::from("app", "Daytrip", "Daytrip")`. The default configuration path is
`~/Library/Application Support/app.Daytrip.Daytrip` on macOS,
`$XDG_CONFIG_HOME/daytrip` or `~/.config/daytrip` on Linux, and
`%APPDATA%\Daytrip\Daytrip\config` on Windows. The complete directory includes
preferences, recovery entries, remembered workspaces, bundled samples and their
assets.

`DOSTY_CONFIG_DIR` overrides that directory. `KOAN_CONFIG_DIR` and then
`DAYTRIP_CONFIG_DIR` remain fallbacks when the Dosty override is unset. An explicit override selects one complete
data location; it does not copy or merge another directory. Use a temporary directory
for isolated validation.

Document metadata keeps its existing names: `.note.md.daytrip`,
`.note.md.daytrip-journal` and `.note.md.daytrip-lock`. Recovery entries retain
`.daytripdraft`. Keeping the shared checkpoint and lock namespace preserves
collaboration identity and the existing file guards across the name change.
Binary signatures, checksums, document UUIDs, CRDT updates and in-memory
`daytrip/` transaction labels are unchanged. No user files are renamed or removed.

New offline exports suggest `.dostyreplica`. All three extensions—`.dostyreplica`,
`.koanreplica` and `.daytripreplica`—appear in the import/open filters and use the same binary format.

Benchmark controls are `DOSTY_PROFILE_BLOCKS`, `DOSTY_PROFILE_SAMPLES`,
`DOSTY_PROFILE_COLD_ONLY` and `DOSTY_IMPORT_SAMPLES`, with their `KOAN_` and then
`DAYTRIP_` equivalents retained as fallbacks. The Dosty setting takes precedence.
Dated validation records retain the names and commands they recorded; see the
[evidence policy](history.md#application-identity-in-retained-evidence).

## Earlier development migration (before Daytrip)

The instructions below apply only to installations predating the Daytrip identity.
They are not steps required to move from Daytrip or Koan to Dosty.

Earlier development installations need a manual transition. Close all editor
windows first and back up the complete configuration directory, Markdown files
and hidden sidecars. Do not run old and new builds against the same document:
their differently named lock files cannot coordinate access.

1. Resolve any interrupted save using the earlier build before migrating. Keep
   conflicting journals and checkpoints for recovery; do not discard them.
2. Copy the earlier configuration directory's contents, including preferences,
   bundled samples, assets and recovery entries, into the new Daytrip location.
   Alternatively, point `DAYTRIP_CONFIG_DIR` to that copied directory. On macOS
   the default is `~/Library/Application Support/app.Daytrip.Daytrip`; on Linux,
   `$XDG_CONFIG_HOME/daytrip` or `~/.config/daytrip`; on Windows,
   `%APPDATA%\Daytrip\Daytrip\config`.
3. For `note.md`, rename the earlier checkpoint sidecar to `.note.md.daytrip`
   and its cooperative lock to `.note.md.daytrip-lock`. Preserve the complete
   files. Never overwrite an existing destination. Normal migration should have
   no pending journal; the current journal name is `.note.md.daytrip-journal`.
4. Rename recovery entries to use `.daytripdraft`, keeping each UUID stem and
   file contents. Exported replica files use the suggested `.daytripreplica`
   extension; their contents remain compatible.
5. Open the migrated copy and confirm its text, collaboration identity, recovery
   list and preferences before making edits. Keep the backup until verified.

Binary checkpoint, journal and recovery signatures and checksums are unchanged;
only their file locations/names change. CRDT writer IDs, document UUIDs, updates,
retained history and relative Markdown assets are preserved by copying complete
files. In-memory transaction/observer labels use `daytrip/`; they are not stored
in the Yjs update format. Do not open a source file before migrating its checkpoint:
without the correctly named metadata, it is treated as an ordinary Markdown import.

Historical text logs use the normalized public identity; see the
[evidence policy](history.md#application-identity-in-retained-evidence).
