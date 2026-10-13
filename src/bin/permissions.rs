use kdl::{KdlDocument, KdlNode};
use std::env;
use std::error::Error;
use std::fs;
use std::io::{ErrorKind, Write};
use std::path::Path;
use std::process::ExitCode;

const PERMISSIONS: [&str; 3] = [
    "WebAccess",
    "ReadApplicationState",
    "ChangeApplicationState",
];

fn main() -> ExitCode {
    let args: Vec<_> = env::args_os().skip(1).collect();
    let [cache, plugin] = args.as_slice() else {
        eprintln!("Usage: silibar-permissions <permissions.kdl> <installed-plugin.wasm>");
        return ExitCode::FAILURE;
    };
    match approve_permissions(Path::new(cache), Path::new(plugin)) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!(
                "Could not approve Silibar permissions in {}: {error}",
                Path::new(cache).display()
            );
            ExitCode::FAILURE
        }
    }
}

fn approve_permissions(cache: &Path, plugin: &Path) -> Result<(), Box<dyn Error>> {
    let plugin = fs::canonicalize(plugin)?;
    let plugin = plugin.to_str().ok_or("Plugin path is not valid UTF-8")?;
    let cache = match fs::canonicalize(cache) {
        Ok(path) => path,
        Err(error) if error.kind() == ErrorKind::NotFound => cache.to_path_buf(),
        Err(error) => return Err(error.into()),
    };
    let original = match fs::read_to_string(&cache) {
        Ok(contents) => contents,
        Err(error) if error.kind() == ErrorKind::NotFound => String::new(),
        Err(error) => return Err(error.into()),
    };
    let mut document: KdlDocument = original.parse()?;
    let keys = [plugin.to_owned(), format!("file:{plugin}")];
    document
        .nodes_mut()
        .retain(|node| !keys.iter().any(|key| key == node.name().value()));
    for key in keys {
        let mut children = KdlDocument::new();
        for permission in PERMISSIONS {
            children.nodes_mut().push(KdlNode::new(permission));
        }
        let mut node = KdlNode::new(key);
        node.set_children(children);
        node.fmt();
        node.set_leading("\n");
        document.nodes_mut().push(node);
    }

    let updated = document.to_string();
    if updated == original {
        return Ok(());
    }
    let parent = cache
        .parent()
        .filter(|path| !path.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    fs::create_dir_all(parent)?;
    let mut temporary = tempfile::NamedTempFile::new_in(parent)?;
    if let Ok(metadata) = fs::metadata(&cache) {
        temporary
            .as_file()
            .set_permissions(metadata.permissions())?;
    }
    temporary.write_all(updated.as_bytes())?;
    temporary.as_file().sync_all()?;
    temporary.persist(&cache)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use zellij_utils::data::PermissionType;
    use zellij_utils::input::permission::PermissionCache;

    fn fixture() -> (tempfile::TempDir, PathBuf, PathBuf) {
        let directory =
            tempfile::tempdir_in(Path::new(env!("CARGO_MANIFEST_DIR")).join("target")).unwrap();
        let plugin = directory.path().join("Silibar \"quoted\" \\ plugin.wasm");
        fs::write(&plugin, b"wasm").unwrap();
        let cache = directory.path().join("cache/permissions.kdl");
        (directory, cache, plugin)
    }

    fn assert_approved(cache: &Path, plugin: &Path) {
        let granted = PermissionCache::from_string(fs::read_to_string(cache).unwrap()).unwrap();
        let path = fs::canonicalize(plugin)
            .unwrap()
            .to_str()
            .unwrap()
            .to_owned();
        for key in [&path, &format!("file:{path}")] {
            let permissions = &granted[key];
            assert_eq!(permissions.len(), 3);
            for permission in [
                PermissionType::WebAccess,
                PermissionType::ReadApplicationState,
                PermissionType::ChangeApplicationState,
            ] {
                assert!(permissions.contains(&permission));
            }
        }
    }

    #[test]
    fn creates_cache_with_permissions_zellij_recognizes_and_reinstall_is_idempotent() {
        let (_directory, cache, plugin) = fixture();
        approve_permissions(&cache, &plugin).unwrap();
        assert_approved(&cache, &plugin);
        let original = fs::read(&cache).unwrap();
        approve_permissions(&cache, &plugin).unwrap();
        assert_eq!(fs::read(&cache).unwrap(), original);
    }

    #[test]
    fn replaces_duplicate_entries_and_preserves_other_plugins() {
        let (_directory, cache, plugin) = fixture();
        fs::create_dir_all(cache.parent().unwrap()).unwrap();
        let unrelated = "// Another plugin\n\"file:/other.wasm\" { RunCommands; }\n";
        let mut key = KdlNode::new(fs::canonicalize(&plugin).unwrap().to_str().unwrap());
        key.set_children("WebAccess; RunCommands;\n".parse().unwrap());
        key.fmt();
        fs::write(&cache, format!("{unrelated}{key}\n{key}\n")).unwrap();
        approve_permissions(&cache, &plugin).unwrap();
        assert_approved(&cache, &plugin);
        let updated = fs::read_to_string(&cache).unwrap();
        assert!(updated.starts_with(unrelated));
        assert_eq!(updated.parse::<KdlDocument>().unwrap().nodes().len(), 3);
        approve_permissions(&cache, &plugin).unwrap();
        assert_eq!(fs::read_to_string(&cache).unwrap(), updated);
    }

    #[test]
    fn malformed_cache_is_left_untouched() {
        let (_directory, cache, plugin) = fixture();
        fs::create_dir_all(cache.parent().unwrap()).unwrap();
        let malformed = "\"file:/other.wasm\" {\n";
        fs::write(&cache, malformed).unwrap();
        assert!(approve_permissions(&cache, &plugin).is_err());
        assert_eq!(fs::read_to_string(&cache).unwrap(), malformed);
    }

    #[test]
    #[cfg(unix)]
    fn preserves_symlinks_and_file_permissions() {
        use std::os::unix::fs::{PermissionsExt, symlink};

        let (directory, cache, plugin) = fixture();
        fs::create_dir_all(cache.parent().unwrap()).unwrap();
        let target = directory.path().join("permissions.kdl");
        fs::write(&target, "").unwrap();
        fs::set_permissions(&target, fs::Permissions::from_mode(0o640)).unwrap();
        symlink(&target, &cache).unwrap();
        approve_permissions(&cache, &plugin).unwrap();
        assert_approved(&target, &plugin);
        assert!(fs::symlink_metadata(&cache).unwrap().is_symlink());
        assert_eq!(
            fs::metadata(&target).unwrap().permissions().mode() & 0o777,
            0o640
        );
    }
}
