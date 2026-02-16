use std::fmt::{Display, Formatter};
use std::path::Path;

#[derive(Debug, PartialEq, Eq)]
pub struct Migration {
    pub version: u32,
    pub name: String,
    pub sql: String,
}

#[derive(Debug)]
pub enum MigrationParseError {
    Read(std::io::Error),
    InvalidExtension,
    InvalidFilename,
    InvalidVersion(std::num::ParseIntError),
}

impl Display for MigrationParseError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Read(err) => write!(f, "failed to read migration file: {err}"),
            Self::InvalidExtension => write!(f, "migration file extension must be .sql"),
            Self::InvalidFilename => {
                write!(f, "migration filename must be <VERSION>_<NAME>.sql")
            }
            Self::InvalidVersion(err) => write!(f, "invalid migration version: {err}"),
        }
    }
}

impl std::error::Error for MigrationParseError {}

impl Migration {
    pub fn from_file(path: impl AsRef<Path>) -> Result<Self, MigrationParseError> {
        let path = path.as_ref();
        let is_sql = path
            .extension()
            .and_then(|ext| ext.to_str())
            .map(|ext| ext.eq_ignore_ascii_case("sql"))
            .unwrap_or(false);
        if !is_sql {
            return Err(MigrationParseError::InvalidExtension);
        }

        let stem = path
            .file_stem()
            .and_then(|stem| stem.to_str())
            .ok_or(MigrationParseError::InvalidFilename)?;
        let (version_str, name) = stem
            .split_once('_')
            .ok_or(MigrationParseError::InvalidFilename)?;
        if version_str.is_empty() || name.is_empty() {
            return Err(MigrationParseError::InvalidFilename);
        }

        // Parse u32 directly so zero-padded versions are naturally accepted.
        let version = version_str
            .parse::<u32>()
            .map_err(MigrationParseError::InvalidVersion)?;
        let sql = std::fs::read_to_string(path).map_err(MigrationParseError::Read)?;

        Ok(Self {
            version,
            name: name.to_string(),
            sql,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn from_file_parses_zero_padded_version() {
        let temp_dir = tempdir().expect("create temp dir");
        let path = temp_dir.path().join("0001_create_accounts.sql");
        let sql = "CREATE TABLE accounts(id INTEGER PRIMARY KEY);";
        std::fs::write(&path, sql).expect("write migration");

        let migration = Migration::from_file(&path).expect("parse migration");

        assert_eq!(migration.version, 1);
        assert_eq!(migration.name, "create_accounts");
        assert_eq!(migration.sql, sql);
    }

    #[test]
    fn from_file_rejects_non_sql_extension() {
        let temp_dir = tempdir().expect("create temp dir");
        let path = temp_dir.path().join("0001_create_accounts.txt");
        std::fs::write(&path, "SELECT 1;").expect("write migration");

        let err =
            Migration::from_file(&path).expect_err("non-sql migration extension should fail");

        assert!(matches!(err, MigrationParseError::InvalidExtension));
    }
}
