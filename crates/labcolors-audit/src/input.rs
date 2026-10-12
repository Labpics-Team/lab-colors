use std::fmt;
use std::fs::{self, DirEntry};
use std::io;
use std::path::{Path, PathBuf};

/// Ошибка обязательного входа аудита с точным путём и причиной.
#[derive(Debug)]
pub struct AuditInputError {
    path: PathBuf,
    source: io::Error,
}

impl AuditInputError {
    pub(crate) fn new(path: &Path, source: io::Error) -> Self {
        Self {
            path: path.to_path_buf(),
            source,
        }
    }

    pub(crate) fn invalid(path: &Path, reason: impl Into<String>) -> Self {
        Self::new(
            path,
            io::Error::new(io::ErrorKind::InvalidData, reason.into()),
        )
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn kind(&self) -> io::ErrorKind {
        self.source.kind()
    }
}

impl fmt::Display for AuditInputError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}: {}", self.path.display(), self.source)
    }
}

impl std::error::Error for AuditInputError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(&self.source)
    }
}

pub(crate) type InputResult<T> = Result<T, AuditInputError>;

/// Обход не следует ссылкам и сохраняет ошибки вместо неполного списка.
pub(crate) fn directory(path: &Path) -> InputResult<Vec<DirEntry>> {
    let metadata = fs::symlink_metadata(path).map_err(|error| AuditInputError::new(path, error))?;
    if !metadata.is_dir() {
        return Err(AuditInputError::invalid(path, "ожидался обычный каталог"));
    }
    let mut entries = fs::read_dir(path)
        .map_err(|error| AuditInputError::new(path, error))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| AuditInputError::new(path, error))?;
    entries.sort_by_key(DirEntry::file_name);
    Ok(entries)
}

pub(crate) fn text(path: &Path) -> InputResult<String> {
    let metadata = fs::symlink_metadata(path).map_err(|error| AuditInputError::new(path, error))?;
    if !metadata.is_file() {
        return Err(AuditInputError::invalid(
            path,
            "ожидался обычный текстовый файл",
        ));
    }
    fs::read_to_string(path).map_err(|error| AuditInputError::new(path, error))
}

pub(crate) fn relative(root: &Path, path: &Path) -> InputResult<String> {
    let relative = path
        .strip_prefix(root)
        .map_err(|_| AuditInputError::invalid(path, "путь вне дерева источников"))?;
    let value = relative
        .to_str()
        .ok_or_else(|| AuditInputError::invalid(path, "путь не является UTF-8"))?;
    Ok(value.replace('\\', "/"))
}

pub(crate) fn crate_directories(root: &Path) -> InputResult<Vec<PathBuf>> {
    let workspace = root.join("Cargo.toml");
    if manifest(&workspace)?.get("workspace").is_none() {
        return Err(AuditInputError::invalid(
            &workspace,
            "отсутствует [workspace]",
        ));
    }
    let path = root.join("crates");
    let mut crates = Vec::new();
    for entry in directory(&path)? {
        let kind = entry
            .file_type()
            .map_err(|error| AuditInputError::new(&entry.path(), error))?;
        if kind.is_symlink() {
            return Err(AuditInputError::invalid(
                &entry.path(),
                "ссылка в дереве источников",
            ));
        }
        if kind.is_dir() {
            let cargo = entry.path().join("Cargo.toml");
            let value = manifest(&cargo)?;
            if value
                .get("package")
                .and_then(|package| package.get("name"))
                .and_then(toml::Value::as_str)
                .is_none()
            {
                return Err(AuditInputError::invalid(&cargo, "отсутствует package.name"));
            }
            crates.push(entry.path());
        }
    }
    if crates.is_empty() {
        return Err(AuditInputError::invalid(&path, "нет исходных пакетов"));
    }
    Ok(crates)
}

pub(crate) fn rust_files(root: &Path) -> InputResult<Vec<PathBuf>> {
    let mut pending = crate_directories(root)?;
    let mut files = Vec::new();
    while let Some(dir) = pending.pop() {
        for entry in directory(&dir)? {
            let path = entry.path();
            let kind = entry
                .file_type()
                .map_err(|error| AuditInputError::new(&path, error))?;
            let name = entry.file_name();
            if kind.is_symlink() {
                return Err(AuditInputError::invalid(
                    &path,
                    "ссылка в дереве источников",
                ));
            }
            if kind.is_dir() {
                if name != "tests" && name != "benches" {
                    pending.push(path);
                }
                continue;
            }
            if path.extension().is_some_and(|extension| extension == "rs") {
                let name = name
                    .to_str()
                    .ok_or_else(|| AuditInputError::invalid(&path, "путь не является UTF-8"))?;
                if name != "build.rs" && !name.ends_with("_test.rs") && !name.ends_with("_tests.rs")
                {
                    files.push(path);
                }
            }
        }
    }
    files.sort();
    if files.is_empty() {
        return Err(AuditInputError::invalid(root, "нет исходников Rust"));
    }
    Ok(files)
}

pub(crate) fn rust(path: &Path, source: &str) -> InputResult<syn::File> {
    syn::parse_file(source).map_err(|error| AuditInputError::invalid(path, error.to_string()))
}

pub(crate) fn manifest(path: &Path) -> InputResult<toml::Value> {
    text(path)?
        .parse()
        .map_err(|error: toml::de::Error| AuditInputError::invalid(path, error.to_string()))
}
