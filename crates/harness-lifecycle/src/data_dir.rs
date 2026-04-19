use std::fs;
use std::io;
use std::path::{Path, PathBuf};

#[must_use]
pub fn data_dir() -> PathBuf {
    resolve_data_dir(
        std::env::var("HARNESS_DATA_DIR").ok(),
        std::env::var("HOME").ok(),
    )
}

fn resolve_data_dir(override_dir: Option<String>, home: Option<String>) -> PathBuf {
    if let Some(p) = override_dir {
        return PathBuf::from(p);
    }
    PathBuf::from(home.unwrap_or_else(|| ".".into())).join(".harness")
}

pub struct DataDir {
    pub root: PathBuf,
}

impl DataDir {
    pub fn init(path: impl AsRef<Path>) -> io::Result<Self> {
        let root = path.as_ref().to_path_buf();
        fs::create_dir_all(&root)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = fs::metadata(&root)?.permissions();
            perms.set_mode(0o700);
            fs::set_permissions(&root, perms)?;
        }
        Ok(Self { root })
    }

    #[must_use]
    pub fn db_path(&self) -> PathBuf {
        self.root.join("harness.db")
    }

    #[must_use]
    pub fn socket_path(&self) -> PathBuf {
        self.root.join("harness.sock")
    }

    #[must_use]
    pub fn backup_path(&self) -> PathBuf {
        self.root.join("harness.db.bak")
    }

    #[must_use]
    pub fn tls_cert_path(&self) -> PathBuf {
        self.root.join("tls").join("cert.pem")
    }

    #[must_use]
    pub fn tls_key_path(&self) -> PathBuf {
        self.root.join("tls").join("key.pem")
    }

    #[must_use]
    pub fn pid_path(&self) -> PathBuf {
        self.root.join("harnessd.pid")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn init_creates_directory() {
        let t = TempDir::new().unwrap();
        let dd = DataDir::init(t.path().join("harness")).unwrap();
        assert!(dd.root.exists());
        assert!(dd.root.is_dir());
    }

    #[cfg(unix)]
    #[test]
    fn init_sets_700_permissions() {
        use std::os::unix::fs::PermissionsExt;
        let t = TempDir::new().unwrap();
        let dd = DataDir::init(t.path().join("harness")).unwrap();
        let perms = fs::metadata(&dd.root).unwrap().permissions();
        assert_eq!(perms.mode() & 0o777, 0o700);
    }

    #[test]
    fn paths_are_under_root() {
        let t = TempDir::new().unwrap();
        let dd = DataDir::init(t.path().join("harness")).unwrap();
        assert!(dd.db_path().starts_with(&dd.root));
        assert!(dd.socket_path().starts_with(&dd.root));
    }

    #[test]
    fn override_dir_wins_over_home() {
        let resolved = resolve_data_dir(Some("/tmp/harness-test".into()), Some("/home/u".into()));
        assert_eq!(resolved, PathBuf::from("/tmp/harness-test"));
    }

    #[test]
    fn falls_back_to_home_when_no_override() {
        let resolved = resolve_data_dir(None, Some("/home/u".into()));
        assert_eq!(resolved, PathBuf::from("/home/u/.harness"));
    }

    #[test]
    fn falls_back_to_cwd_when_no_home() {
        let resolved = resolve_data_dir(None, None);
        assert_eq!(resolved, PathBuf::from("./.harness"));
    }
}
