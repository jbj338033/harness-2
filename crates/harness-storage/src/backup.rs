use crate::Result;
use rusqlite::{Connection, backup::Backup};
use std::path::Path;
use std::time::Duration;

pub fn run(src: &Connection, dst_path: impl AsRef<Path>) -> Result<()> {
    let mut dst = Connection::open(dst_path.as_ref())?;
    let backup = Backup::new(src, &mut dst)?;
    backup.run_to_completion(100, Duration::from_millis(50), None)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{db, migrations};
    use tempfile::NamedTempFile;

    #[test]
    fn backup_copies_schema_and_data() {
        let src_file = NamedTempFile::new().unwrap();
        let dst_file = NamedTempFile::new().unwrap();

        let mut src = Connection::open(src_file.path()).unwrap();
        db::configure(&mut src).unwrap();
        migrations::apply(&mut src).unwrap();
        src.execute("INSERT INTO config (key, value) VALUES ('k', 'v')", [])
            .unwrap();

        run(&src, dst_file.path()).unwrap();

        let dst = Connection::open(dst_file.path()).unwrap();
        let value: String = dst
            .query_row("SELECT value FROM config WHERE key = 'k'", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(value, "v");
    }
}
