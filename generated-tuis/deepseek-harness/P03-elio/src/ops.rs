//! Filesystem operations. Every operation talks to the real filesystem;
//! nothing here is simulated.

use std::fs;
use std::path::{Path, PathBuf};

/// A single directory entry shown in the file list.
#[derive(Clone)]
pub struct Entry {
    pub name: String,
    pub path: PathBuf,
    pub is_dir: bool,
    pub size: u64,
}

/// Content captured for the preview panel.
#[derive(Clone)]
pub enum Preview {
    /// Valid UTF-8 file content, shown verbatim.
    Text(String),
    /// Non-UTF-8 file: we show the size plus a hex dump of the head.
    Binary { size: u64, hex: String },
    /// An existing file with zero bytes.
    Empty,
    /// The file could not be read.
    Error(String),
}

/// List the entries of `dir` (unsorted; callers sort).
pub fn list_dir(dir: &Path) -> Result<Vec<Entry>, String> {
    let rd = fs::read_dir(dir)
        .map_err(|e| format!("Cannot read directory {}: {}", dir.display(), e))?;
    let mut out = Vec::new();
    for item in rd {
        let item = item.map_err(|e| format!("Cannot read entry: {}", e))?;
        let path = item.path();
        let name = item.file_name().to_string_lossy().to_string();
        let (is_dir, size) = match fs::metadata(&path) {
            Ok(m) => (m.is_dir(), m.len()),
            Err(_) => (false, 0),
        };
        out.push(Entry {
            name,
            path,
            is_dir,
            size,
        });
    }
    Ok(out)
}

/// Copy a file byte-for-byte. Uses the real `std::fs::copy`.
pub fn copy_file(src: &Path, dst: &Path) -> Result<(), String> {
    fs::copy(src, dst).map_err(|e| format!("Copy failed: {}", e))?;
    Ok(())
}

/// Rename a path (original disappears, content unchanged).
pub fn rename_path(src: &Path, dst: &Path) -> Result<(), String> {
    fs::rename(src, dst).map_err(|e| format!("Rename failed: {}", e))?;
    Ok(())
}

/// Move a path to `dst`. Falls back to copy + delete for cross-device moves
/// of regular files; cross-device directory moves are reported as an error.
pub fn move_path(src: &Path, dst: &Path) -> Result<(), String> {
    if let Err(rename_err) = fs::rename(src, dst) {
        if fs::metadata(src).map(|m| m.is_dir()).unwrap_or(false) {
            return Err(format!(
                "Move failed: {} (cross-device directory move not supported)",
                rename_err
            ));
        }
        copy_file(src, dst).map_err(|e| {
            format!(
                "Move failed: {} (copy fallback also failed: {})",
                rename_err, e
            )
        })?;
        fs::remove_file(src).map_err(|e| {
            format!(
                "Move copied but could not remove source {}: {}",
                src.display(),
                e
            )
        })?;
    }
    Ok(())
}

/// Delete a file (or a directory, recursively).
pub fn delete_path(path: &Path) -> Result<(), String> {
    let md = fs::metadata(path).map_err(|e| format!("Cannot access {}: {}", path.display(), e))?;
    if md.is_dir() {
        fs::remove_dir_all(path).map_err(|e| format!("Delete failed: {}", e))
    } else {
        fs::remove_file(path).map_err(|e| format!("Delete failed: {}", e))
    }
}

/// Create a directory (including any missing parents).
pub fn make_dir(path: &Path) -> Result<(), String> {
    fs::create_dir_all(path).map_err(|e| format!("Create directory failed: {}", e))?;
    Ok(())
}

/// Read a file for the preview panel.
pub fn read_preview(path: &Path) -> Result<Preview, String> {
    let data = fs::read(path).map_err(|e| format!("Read failed: {}", e))?;
    if data.is_empty() {
        return Ok(Preview::Empty);
    }
    match String::from_utf8(data) {
        Ok(s) => Ok(Preview::Text(s)),
        Err(e) => {
            let bytes = e.into_bytes();
            let size = bytes.len() as u64;
            let hex = hex_dump(&bytes, 8192);
            Ok(Preview::Binary { size, hex })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn tempdir(tag: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("toolc-test-{}-{}", tag, nanos));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn list_dir_reports_names_and_types() {
        let d = tempdir("list");
        fs::write(d.join("a.txt"), b"x").unwrap();
        fs::create_dir(d.join("sub")).unwrap();

        let entries = list_dir(&d).unwrap();
        let names: Vec<&str> = entries.iter().map(|e| e.name.as_str()).collect();
        assert!(names.contains(&"a.txt"));
        assert!(names.contains(&"sub"));
        let sub = entries.iter().find(|e| e.name == "sub").unwrap();
        assert!(sub.is_dir);
        let file = entries.iter().find(|e| e.name == "a.txt").unwrap();
        assert!(!file.is_dir);
        assert_eq!(file.size, 1);
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn copy_is_byte_identical() {
        let d = tempdir("copy");
        let src = d.join("src.bin");
        let data: Vec<u8> = (0..=255u8).collect();
        fs::write(&src, &data).unwrap();
        let dst = d.join("dst.bin");
        copy_file(&src, &dst).unwrap();
        assert_eq!(fs::read(&dst).unwrap(), data);
        assert!(src.exists());
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn rename_removes_original() {
        let d = tempdir("rename");
        let src = d.join("old.txt");
        fs::write(&src, b"content").unwrap();
        let dst = d.join("new.txt");
        rename_path(&src, &dst).unwrap();
        assert!(!src.exists());
        assert_eq!(fs::read(&dst).unwrap(), b"content");
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn move_removes_original() {
        let d = tempdir("move");
        fs::create_dir(d.join("dest")).unwrap();
        let src = d.join("file.txt");
        fs::write(&src, b"payload").unwrap();
        let target = d.join("dest").join("file.txt");
        move_path(&src, &target).unwrap();
        assert!(!src.exists());
        assert_eq!(fs::read(&target).unwrap(), b"payload");
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn delete_file_and_directory() {
        let d = tempdir("delete");
        fs::write(d.join("f"), b"x").unwrap();
        delete_path(&d.join("f")).unwrap();
        assert!(!d.join("f").exists());

        fs::create_dir_all(d.join("a").join("b")).unwrap();
        fs::write(d.join("a").join("b").join("c"), b"y").unwrap();
        delete_path(&d.join("a")).unwrap();
        assert!(!d.join("a").exists());
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn make_dir_nested() {
        let d = tempdir("mkdir");
        let target = d.join("archive").join("2024");
        make_dir(&target).unwrap();
        assert!(target.is_dir());
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn preview_text_and_binary() {
        let d = tempdir("preview");
        let txt = d.join("t.txt");
        fs::write(&txt, "hello\nworld").unwrap();
        match read_preview(&txt).unwrap() {
            Preview::Text(s) => assert_eq!(s, "hello\nworld"),
            _ => panic!("expected text preview"),
        }

        let bin = d.join("b.bin");
        let mut bytes = vec![0u8, 1, 2, 0xff, 0xfe, 0x00];
        bytes.extend_from_slice(&[b'A'; 100]);
        fs::write(&bin, &bytes).unwrap();
        match read_preview(&bin).unwrap() {
            Preview::Binary { size, hex } => {
                assert_eq!(size, bytes.len() as u64);
                assert!(hex.contains("|"));
            }
            _ => panic!("expected binary preview"),
        }

        let empty = d.join("e.txt");
        fs::write(&empty, b"").unwrap();
        assert!(matches!(read_preview(&empty).unwrap(), Preview::Empty));
        let _ = fs::remove_dir_all(&d);
    }
}

/// A compact `hexdump -C`-style rendering of `data`, capped at `limit` bytes.
fn hex_dump(data: &[u8], limit: usize) -> String {
    let mut out = String::new();
    let n = data.len().min(limit);
    for (i, chunk) in data[..n].chunks(16).enumerate() {
        out.push_str(&format!("{:08x}  ", i * 16));
        let mut hex_part = String::new();
        for (j, b) in chunk.iter().enumerate() {
            if j == 8 {
                hex_part.push(' ');
            }
            hex_part.push_str(&format!("{:02x} ", b));
        }
        while hex_part.len() < 49 {
            hex_part.push(' ');
        }
        let ascii: String = chunk
            .iter()
            .map(|&b| if (0x20..0x7f).contains(&b) { b as char } else { '.' })
            .collect();
        out.push_str(&hex_part);
        out.push_str(&format!(" |{}|", ascii));
        out.push('\n');
    }
    if data.len() > n {
        out.push_str(&format!("... ({} more bytes not shown)\n", data.len() - n));
    }
    out
}
