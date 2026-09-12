//! Adopting the profile written before the rename.
//!
//! The desk used to be called System Design Roulette, and its data lived under
//! that bundle identifier in a file called `roulette.db`. Both changed with the
//! product name. A learner who upgrades must not silently start again with an
//! empty desk, so the first launch under the new identity looks for the old
//! profile and takes a consolidated copy of it.
//!
//! The copy goes through SQLite's backup API rather than the filesystem, so a
//! database whose recent writes are still in its write-ahead log arrives whole.
//! The original is left where it is: if anything about the new profile looks
//! wrong, the record it came from is still there.

use crate::db::Result;
use rusqlite::{Connection, DatabaseName, OpenFlags};
use std::path::{Path, PathBuf};

/// Bundle identifier used before the rename.
pub const LEGACY_IDENTIFIER: &str = "com.darkmatter.system-design-roulette";
/// Database file name used before the rename.
pub const LEGACY_DATABASE: &str = "roulette.db";

/// Where a profile from before the rename could be, nearest first: the same
/// directory under the old file name, then the old identifier's directory.
pub fn previous_locations(data_dir: &Path) -> Vec<PathBuf> {
    let mut found = vec![data_dir.join(LEGACY_DATABASE)];
    if let Some(parent) = data_dir.parent() {
        found.push(parent.join(LEGACY_IDENTIFIER).join(LEGACY_DATABASE));
    }
    found
}

/// The first previous profile that exists and holds something.
pub fn previous_profile(data_dir: &Path) -> Option<PathBuf> {
    previous_locations(data_dir)
        .into_iter()
        .find(|path| path.exists() && path.metadata().map(|m| m.len() > 0).unwrap_or(false))
}

/// Copy a profile written before the rename into `database`, if this build has
/// none of its own yet. Returns the path it was taken from.
pub fn adopt(data_dir: &Path, database: &Path) -> Result<Option<PathBuf>> {
    if database.exists() {
        return Ok(None);
    }
    let Some(source) = previous_profile(data_dir) else {
        return Ok(None);
    };
    if let Some(parent) = database.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let previous = Connection::open_with_flags(&source, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
    previous.busy_timeout(std::time::Duration::from_secs(5))?;
    previous.backup(DatabaseName::Main, database, None)?;
    Ok(Some(source))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "principia-adoption-{name}-{}-{:x}",
            std::process::id(),
            rand::random::<u64>()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn the_profile_written_before_the_rename_is_adopted_once_and_left_in_place() {
        let root = scratch("root");
        let old_dir = root.join(LEGACY_IDENTIFIER);
        let new_dir = root.join("com.darkmatter.principia-desk");
        std::fs::create_dir_all(&old_dir).unwrap();
        std::fs::create_dir_all(&new_dir).unwrap();

        let old = old_dir.join(LEGACY_DATABASE);
        let source = Connection::open(&old).unwrap();
        source
            .execute_batch(
                "CREATE TABLE lessons (title TEXT); INSERT INTO lessons VALUES ('the event loop');",
            )
            .unwrap();
        // Writes still in the log must arrive with the rest of the record.
        source.pragma_update(None, "journal_mode", "WAL").unwrap();
        source
            .execute("INSERT INTO lessons VALUES ('closures')", [])
            .unwrap();
        drop(source);

        let database = new_dir.join("principia.db");
        let taken = adopt(&new_dir, &database).unwrap();
        assert_eq!(taken.as_deref(), Some(old.as_path()));
        assert!(old.exists(), "the original record is never removed");

        let adopted = Connection::open(&database).unwrap();
        let titles: Vec<String> = adopted
            .prepare("SELECT title FROM lessons ORDER BY title")
            .unwrap()
            .query_map([], |row| row.get(0))
            .unwrap()
            .collect::<std::result::Result<_, _>>()
            .unwrap();
        assert_eq!(titles, vec!["closures", "the event loop"]);
        drop(adopted);

        // A second launch keeps the profile this build already has.
        assert_eq!(adopt(&new_dir, &database).unwrap(), None);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn a_renamed_file_in_the_same_directory_is_found_too() {
        let dir = scratch("same");
        let old = dir.join(LEGACY_DATABASE);
        Connection::open(&old)
            .unwrap()
            .execute_batch("CREATE TABLE t (a INTEGER)")
            .unwrap();
        assert_eq!(previous_profile(&dir).as_deref(), Some(old.as_path()));

        let fresh = scratch("fresh");
        assert_eq!(previous_profile(&fresh), None);
        assert_eq!(adopt(&fresh, &fresh.join("principia.db")).unwrap(), None);
        std::fs::remove_dir_all(dir).unwrap();
        std::fs::remove_dir_all(fresh).unwrap();
    }
}
