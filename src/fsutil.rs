use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::Path;
use std::process;
use std::time::{SystemTime, UNIX_EPOCH};

pub fn read_file(path: &Path) -> Result<String, String> {
    fs::read_to_string(path).map_err(|err| format!("failed reading {}: {err}", path.display()))
}

pub fn atomic_write(path: &Path, content: &str) -> Result<(), String> {
    let parent = path
        .parent()
        .ok_or_else(|| format!("missing parent directory for {}", path.display()))?;
    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| format!("invalid filename for {}", path.display()))?;

    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|err| err.to_string())?
        .as_nanos();
    let tmp_path = parent.join(format!(".{name}.tmp-{}-{stamp}", process::id()));

    let mut tmp_file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&tmp_path)
        .map_err(|err| format!("failed creating {}: {err}", tmp_path.display()))?;

    if let Ok(metadata) = fs::metadata(path) {
        tmp_file
            .set_permissions(metadata.permissions())
            .map_err(|err| {
                format!(
                    "failed setting permissions on {}: {err}",
                    tmp_path.display()
                )
            })?;
    }

    tmp_file
        .write_all(content.as_bytes())
        .map_err(|err| format!("failed writing {}: {err}", tmp_path.display()))?;
    tmp_file
        .sync_all()
        .map_err(|err| format!("failed syncing {}: {err}", tmp_path.display()))?;
    drop(tmp_file);

    replace_file(&tmp_path, path)?;
    sync_directory(parent)?;
    Ok(())
}

#[cfg(not(windows))]
fn replace_file(source: &Path, target: &Path) -> Result<(), String> {
    fs::rename(source, target)
        .map_err(|err| format!("failed replacing {}: {err}", target.display()))
}

#[cfg(windows)]
fn replace_file(source: &Path, target: &Path) -> Result<(), String> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Storage::FileSystem::{
        MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH, MoveFileExW,
    };

    let source_wide: Vec<u16> = source
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();
    let target_wide: Vec<u16> = target
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();

    let result = unsafe {
        MoveFileExW(
            source_wide.as_ptr(),
            target_wide.as_ptr(),
            MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
        )
    };

    if result == 0 {
        return Err(format!(
            "failed replacing {}: {}",
            target.display(),
            std::io::Error::last_os_error()
        ));
    }

    Ok(())
}

#[cfg(not(windows))]
fn sync_directory(path: &Path) -> Result<(), String> {
    let dir =
        File::open(path).map_err(|err| format!("failed opening {}: {err}", path.display()))?;
    dir.sync_all()
        .map_err(|err| format!("failed syncing {}: {err}", path.display()))
}

#[cfg(windows)]
fn sync_directory(_path: &Path) -> Result<(), String> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;

    #[test]
    fn atomic_write_replaces_contents() {
        let root = env::temp_dir().join(format!(
            "cxc-test-{}-{}",
            process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&root).unwrap();
        let file_path = root.join("sample.txt");
        fs::write(&file_path, "old").unwrap();

        atomic_write(&file_path, "new").unwrap();

        assert_eq!(fs::read_to_string(&file_path).unwrap(), "new");

        fs::remove_file(&file_path).unwrap();
        fs::remove_dir(&root).unwrap();
    }
}
