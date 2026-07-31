use crate::arch::x86::{X86Level, detect_x86_level, native_hasher};
use std::{
    fs::{File, OpenOptions},
    io::{self, Error, ErrorKind, Read, Seek, SeekFrom, Write},
    path::Path,
};

const FOOTER_MAGIC: &[u8; 8] = b"SPZSTD1\0";
const FOOTER_SIZE: u64 = 33;
const IS_LAUNCHED: u8 = 1;
const MIN_ENTRY_SIZE: u64 = 28;
const MAX_NAME_SIZE: usize = 4096;
const MAX_DECOMPRESSED_PAYLOAD_SIZE: u64 = 1024 * 1024 * 1024;
const ZSTD_COMPRESSION_LEVEL: i32 = 15;

/// A single packed payload entry stored in the manifest.
struct Entry {
    name: String,
    offset: u64,
    compressed_size: u64,
    decompressed_size: u64,
}

/// Read a little-endian `u32` from the current file position.
fn read_u32(file: &mut File) -> io::Result<u32> {
    let mut buf = [0u8; 4];
    file.read_exact(&mut buf)?;
    Ok(u32::from_le_bytes(buf))
}

/// Read a little-endian `u64` from the current file position.
fn read_u64(file: &mut File) -> io::Result<u64> {
    let mut buf = [0u8; 8];
    file.read_exact(&mut buf)?;
    Ok(u64::from_le_bytes(buf))
}

/// Build a packed executable by appending payloads and a manifest footer.
pub fn pack_files<P, O>(launcher_path: P, output_path: O, payload_paths: &[String]) -> io::Result<()>
where
    P: AsRef<Path>,
    O: AsRef<Path>,
{
    let entry_count =
        u32::try_from(payload_paths.len()).map_err(|_| Error::new(ErrorKind::InvalidInput, "too many payloads"))?;
    let mut output = OpenOptions::new().create_new(true).write(true).open(output_path)?;

    let mut launcher = File::open(launcher_path)?;
    io::copy(&mut launcher, &mut output)?;

    let mut entries = Vec::with_capacity(payload_paths.len());

    for payload_path in payload_paths {
        let payload_path = Path::new(payload_path);
        let offset = output.stream_position()?;
        let mut payload = File::open(payload_path).map_err(|err| {
            Error::new(
                err.kind(),
                format!("failed to open payload {}: {err}", payload_path.display()),
            )
        })?;
        let decompressed_size = payload.metadata()?.len();

        if decompressed_size > MAX_DECOMPRESSED_PAYLOAD_SIZE {
            return Err(Error::new(
                ErrorKind::InvalidInput,
                "payload exceeds the 1 GiB decompressed-size limit",
            ));
        }

        let mut encoder = zstd::stream::write::Encoder::new(&mut output, ZSTD_COMPRESSION_LEVEL)?;
        encoder.include_checksum(true)?;
        io::copy(&mut payload, &mut encoder)?;
        encoder.finish()?;

        let compressed_size = output.stream_position()? - offset;
        let name = payload_path
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or_else(|| Error::new(ErrorKind::InvalidInput, "payload path has no valid file name"))?
            .to_string();

        if name.len() > MAX_NAME_SIZE {
            return Err(Error::new(ErrorKind::InvalidInput, "payload file name is too long"));
        }

        entries.push(Entry {
            name,
            offset,
            compressed_size,
            decompressed_size,
        });
    }

    // The manifest is appended after the launcher + payload blobs.
    let manifest_offset = output.stream_position()?;
    output.write_all(&entry_count.to_le_bytes())?;

    for entry in &entries {
        let name_bytes = entry.name.as_bytes();
        let name_len = u32::try_from(name_bytes.len())
            .map_err(|_| Error::new(ErrorKind::InvalidInput, "payload file name is too long"))?;
        output.write_all(&name_len.to_le_bytes())?;
        output.write_all(name_bytes)?;
        output.write_all(&entry.offset.to_le_bytes())?;
        output.write_all(&entry.compressed_size.to_le_bytes())?;
        output.write_all(&entry.decompressed_size.to_le_bytes())?;
    }

    let manifest_size = output.stream_position()? - manifest_offset;
    output.write_all(FOOTER_MAGIC)?;
    output.write_all(&manifest_offset.to_le_bytes())?;
    output.write_all(&manifest_size.to_le_bytes())?;

    match native_hasher() {
        Some(v) => output.write_all(&v.to_le_bytes())?,
        None => output.write_all(&0u64.to_le_bytes())?,
    }

    output.write_all(&[IS_LAUNCHED])?;

    Ok(())
}

/// Read the packed file footer, locate the best matching payload, and return it
pub fn read_back<P>(path: P) -> io::Result<Vec<u8>>
where
    P: AsRef<Path>,
{
    let mut file = OpenOptions::new().read(true).open(path)?;

    let file_size = file.metadata()?.len();

    if file_size < FOOTER_SIZE {
        return Err(Error::new(ErrorKind::InvalidData, "file too small"));
    }

    file.seek(SeekFrom::End(-(FOOTER_SIZE as i64)))?;

    let mut magic = [0u8; 8];
    file.read_exact(&mut magic)?;

    if &magic != FOOTER_MAGIC {
        return Err(Error::new(ErrorKind::InvalidData, "invalid footer magic"));
    }

    // Footer layout: magic, manifest offset, manifest size, native hash, launch flag.
    let manifest_offset = read_u64(&mut file)?;
    let manifest_size = read_u64(&mut file)?;
    let mut native_hash = [0u8; 8];
    file.read_exact(&mut native_hash)?;
    let mut launch_flag = [0u8; 1];
    file.read_exact(&mut launch_flag)?;

    if launch_flag[0] != IS_LAUNCHED {
        return Err(Error::new(ErrorKind::InvalidData, "invalid launch flag"));
    }

    let manifest_end = manifest_offset
        .checked_add(manifest_size)
        .ok_or_else(|| Error::new(ErrorKind::InvalidData, "manifest range overflow"))?;
    let footer_offset = file_size - FOOTER_SIZE;

    if manifest_size < 4 || manifest_end != footer_offset {
        return Err(Error::new(ErrorKind::InvalidData, "invalid manifest range"));
    }

    file.seek(SeekFrom::Start(manifest_offset))?;

    let entry_count = read_u32(&mut file)?;

    if u64::from(entry_count) > (manifest_size - 4) / MIN_ENTRY_SIZE {
        return Err(Error::new(ErrorKind::InvalidData, "invalid manifest entry count"));
    }

    // Each manifest entry stores the payload name and its byte range.
    let mut entries = Vec::with_capacity(entry_count as usize);

    for _ in 0..entry_count {
        ensure_available(&mut file, manifest_end, 4)?;
        let name_len = read_u32(&mut file)? as usize;

        if name_len > MAX_NAME_SIZE {
            return Err(Error::new(ErrorKind::InvalidData, "payload file name is too long"));
        }

        let entry_tail = u64::try_from(name_len)
            .ok()
            .and_then(|name_len| name_len.checked_add(24))
            .ok_or_else(|| Error::new(ErrorKind::InvalidData, "manifest entry size overflow"))?;
        ensure_available(&mut file, manifest_end, entry_tail)?;

        let mut name_bytes = vec![0u8; name_len];
        file.read_exact(&mut name_bytes)?;

        let name = String::from_utf8(name_bytes)
            .map_err(|_| Error::new(ErrorKind::InvalidData, "invalid UTF-8 in file name"))?;

        let offset = read_u64(&mut file)?;
        let compressed_size = read_u64(&mut file)?;
        let decompressed_size = read_u64(&mut file)?;

        let payload_end = offset
            .checked_add(compressed_size)
            .ok_or_else(|| Error::new(ErrorKind::InvalidData, "payload range overflow"))?;

        if payload_end > manifest_offset {
            return Err(Error::new(
                ErrorKind::InvalidData,
                "payload range overlaps the manifest",
            ));
        }

        if decompressed_size > MAX_DECOMPRESSED_PAYLOAD_SIZE {
            return Err(Error::new(
                ErrorKind::InvalidData,
                "decompressed payload exceeds the safety limit",
            ));
        }

        entries.push(Entry {
            name,
            offset,
            compressed_size,
            decompressed_size,
        });
    }

    if file.stream_position()? != manifest_end {
        return Err(Error::new(ErrorKind::InvalidData, "manifest contains trailing data"));
    }

    let entry = find_optimal(&entries, &native_hash)?;
    decompress_payload(&mut file, entry)
}

fn decompress_payload(file: &mut File, entry: &Entry) -> io::Result<Vec<u8>> {
    let payload_size = usize::try_from(entry.decompressed_size)
        .map_err(|_| Error::new(ErrorKind::InvalidData, "payload is too large for this platform"))?;
    let mut payload = Vec::new();
    payload
        .try_reserve_exact(payload_size)
        .map_err(|error| Error::other(format!("could not allocate decompressed payload: {error}")))?;
    payload.resize(payload_size, 0);

    file.seek(SeekFrom::Start(entry.offset))?;
    let compressed = file.take(entry.compressed_size);
    let mut decoder = zstd::stream::read::Decoder::new(compressed)
        .map_err(|error| Error::new(ErrorKind::InvalidData, format!("invalid zstd payload: {error}")))?;

    decoder
        .read_exact(&mut payload)
        .map_err(|error| Error::new(ErrorKind::InvalidData, format!("could not decompress payload: {error}")))?;

    let mut extra = [0u8; 1];
    if decoder.read(&mut extra).map_err(|error| {
        Error::new(
            ErrorKind::InvalidData,
            format!("could not finish zstd payload: {error}"),
        )
    })? != 0
    {
        return Err(Error::new(
            ErrorKind::InvalidData,
            "decompressed payload is larger than its manifest size",
        ));
    }

    Ok(payload)
}

fn ensure_available(file: &mut File, end: u64, size: u64) -> io::Result<()> {
    let position = file.stream_position()?;
    let requested_end = position
        .checked_add(size)
        .ok_or_else(|| Error::new(ErrorKind::InvalidData, "manifest range overflow"))?;

    if requested_end > end {
        return Err(Error::new(
            ErrorKind::InvalidData,
            "manifest entry extends beyond the manifest",
        ));
    }

    Ok(())
}

/// Pick the payload that best matches the CPU's supported x86-64 level.
fn find_optimal<'a>(entries: &'a [Entry], native_hash: &[u8]) -> io::Result<&'a Entry> {
    select_optimal(entries, native_hash, detect_x86_level(), native_hasher())
}

fn select_optimal<'a>(
    entries: &'a [Entry],
    native_hash: &[u8],
    level: X86Level,
    current_native_hash: Option<u64>,
) -> io::Result<&'a Entry> {
    if let Some(hash) = current_native_hash
        && hash.to_le_bytes() == native_hash
    {
        for entry in entries {
            let label = payload_label(&entry.name);

            if label_matches(label, "native") {
                return Ok(entry);
            }
        }
    }

    for candidate in [X86Level::V4, X86Level::V3, X86Level::V2, X86Level::X86_64] {
        if candidate > level {
            continue;
        }

        let wanted = match candidate {
            X86Level::V4 => "x86-64-v4",
            X86Level::V3 => "x86-64-v3",
            X86Level::V2 => "x86-64-v2",
            X86Level::X86_64 => "x86-64",
        };
        let wanted_with_underscores = wanted.replace('-', "_");

        for entry in entries {
            let label = payload_label(&entry.name);

            if label_matches(label, wanted) || label_matches(label, &wanted_with_underscores) {
                return Ok(entry);
            }
        }
    }

    Err(io::Error::new(io::ErrorKind::NotFound, "no compatible binary found"))
}

fn payload_label(name: &str) -> &str {
    name.rsplit_once('.')
        .filter(|(_, extension)| extension.eq_ignore_ascii_case("exe"))
        .map_or(name, |(stem, _)| stem)
}

fn label_matches(label: &str, target: &str) -> bool {
    label == target
        || label
            .strip_suffix(target)
            .is_some_and(|prefix| prefix.ends_with(['-', '_']))
}

pub fn is_archive<P>(path: P) -> io::Result<bool>
where
    P: AsRef<Path>,
{
    let mut file = OpenOptions::new().read(true).open(path)?;

    let file_size = file.metadata()?.len();

    if file_size < FOOTER_SIZE {
        return Ok(false);
    }

    file.seek(SeekFrom::End(-(FOOTER_SIZE as i64)))?;

    let mut magic = [0u8; 8];
    file.read_exact(&mut magic)?;

    if &magic == FOOTER_MAGIC {
        file.seek(SeekFrom::End(-1))?;
        let mut is_launched = [0u8; 1];
        file.read_exact(&mut is_launched)?;

        if is_launched[0] == IS_LAUNCHED {
            return Ok(true);
        }
    }

    Ok(false)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::arch::x86::{X86Level, detect_x86_level, native_hasher};
    use std::fs;

    #[test]
    fn normal_file_is_not_archive() -> io::Result<()> {
        let dir = tempfile::tempdir()?;
        let file = dir.path().join("plain-file");

        fs::write(&file, b"hello")?;

        assert!(!is_archive(&file)?);

        Ok(())
    }

    #[test]
    fn packed_file_is_archive() -> io::Result<()> {
        let dir = tempfile::tempdir()?;

        let launcher = dir.path().join("launcher");
        let output = dir.path().join("packed");

        let native = dir.path().join("c-native");
        let x86_64 = dir.path().join("c-x86-64");
        let v2 = dir.path().join("c-x86-64-v2");
        let v3 = dir.path().join("c-x86-64-v3");
        let v4 = dir.path().join("c-x86-64-v4");

        fs::write(&launcher, b"fake launcher")?;
        fs::write(&native, b"native payload")?;
        fs::write(&x86_64, b"x86-64 payload")?;
        fs::write(&v2, b"x86-64-v2 payload")?;
        fs::write(&v3, b"x86-64-v3 payload")?;
        fs::write(&v4, b"x86-64-v4 payload")?;

        let payloads = vec![
            native.display().to_string(),
            x86_64.display().to_string(),
            v2.display().to_string(),
            v3.display().to_string(),
            v4.display().to_string(),
        ];

        pack_files(&launcher, &output, &payloads)?;

        assert!(is_archive(&output)?);

        Ok(())
    }

    #[test]
    fn packed_file_reads_best_payload() -> io::Result<()> {
        let dir = tempfile::tempdir()?;

        let launcher = dir.path().join("launcher");
        let output = dir.path().join("packed");

        let native = dir.path().join("c-native");
        let x86_64 = dir.path().join("c-x86-64");
        let v2 = dir.path().join("c-x86-64-v2");
        let v3 = dir.path().join("c-x86-64-v3");
        let v4 = dir.path().join("c-x86-64-v4");

        fs::write(&launcher, b"fake launcher")?;
        fs::write(&native, b"native payload")?;
        fs::write(&x86_64, b"x86-64 payload")?;
        fs::write(&v2, b"x86-64-v2 payload")?;
        fs::write(&v3, b"x86-64-v3 payload")?;
        fs::write(&v4, b"x86-64-v4 payload")?;

        let payloads = vec![
            native.display().to_string(),
            x86_64.display().to_string(),
            v2.display().to_string(),
            v3.display().to_string(),
            v4.display().to_string(),
        ];

        pack_files(&launcher, &output, &payloads)?;

        let actual = read_back(&output)?;

        let expected: &[u8] = if native_hasher().is_some() {
            b"native payload"
        } else {
            match detect_x86_level() {
                X86Level::X86_64 => b"x86-64 payload",
                X86Level::V2 => b"x86-64-v2 payload",
                X86Level::V3 => b"x86-64-v3 payload",
                X86Level::V4 => b"x86-64-v4 payload",
            }
        };

        assert_eq!(actual, expected);

        Ok(())
    }

    #[test]
    fn selection_falls_back_to_a_lower_level() -> io::Result<()> {
        let entries = vec![
            Entry {
                name: "c-x86-64".to_string(),
                offset: 10,
                compressed_size: 1,
                decompressed_size: 11,
            },
            Entry {
                name: "c-x86-64-v2".to_string(),
                offset: 20,
                compressed_size: 2,
                decompressed_size: 22,
            },
        ];

        let selected = select_optimal(&entries, &[0; 8], X86Level::V4, None)?;
        assert_eq!(
            (selected.offset, selected.compressed_size, selected.decompressed_size),
            (20, 2, 22)
        );
        Ok(())
    }

    #[test]
    fn selection_accepts_windows_executable_names() -> io::Result<()> {
        let entries = vec![Entry {
            name: "rust-x86_64_v3.exe".to_string(),
            offset: 42,
            compressed_size: 7,
            decompressed_size: 70,
        }];

        let selected = select_optimal(&entries, &[0; 8], X86Level::V3, None)?;
        assert_eq!((selected.offset, selected.compressed_size), (42, 7));
        Ok(())
    }

    #[test]
    fn payloads_are_zstd_compressed() -> io::Result<()> {
        let dir = tempfile::tempdir()?;
        let launcher = dir.path().join("launcher");
        let payload = dir.path().join("c-x86-64");
        let output = dir.path().join("packed");
        let payload_bytes = vec![b'A'; 64 * 1024];

        fs::write(&launcher, b"launcher")?;
        fs::write(&payload, &payload_bytes)?;
        pack_files(&launcher, &output, &[payload.display().to_string()])?;

        assert!(fs::metadata(&output)?.len() < payload_bytes.len() as u64);
        assert_eq!(read_back(&output)?, payload_bytes);
        Ok(())
    }

    #[test]
    fn corrupted_zstd_payload_is_rejected() -> io::Result<()> {
        let dir = tempfile::tempdir()?;
        let launcher = dir.path().join("launcher");
        let payload = dir.path().join("c-x86-64");
        let output = dir.path().join("packed");

        fs::write(&launcher, b"launcher")?;
        fs::write(&payload, vec![b'A'; 64 * 1024])?;
        pack_files(&launcher, &output, &[payload.display().to_string()])?;

        let mut file = OpenOptions::new().read(true).write(true).open(&output)?;
        file.seek(SeekFrom::End(-25))?;
        let manifest_offset = read_u64(&mut file)?;
        file.seek(SeekFrom::Start(manifest_offset - 1))?;

        let mut checksum_byte = [0u8; 1];
        file.read_exact(&mut checksum_byte)?;
        checksum_byte[0] ^= 0xff;
        file.seek(SeekFrom::Start(manifest_offset - 1))?;
        file.write_all(&checksum_byte)?;

        assert_eq!(read_back(&output).unwrap_err().kind(), ErrorKind::InvalidData);
        Ok(())
    }

    #[test]
    fn malformed_manifest_range_is_rejected() -> io::Result<()> {
        let dir = tempfile::tempdir()?;
        let launcher = dir.path().join("launcher");
        let payload = dir.path().join("c-x86-64");
        let output = dir.path().join("packed");

        fs::write(&launcher, b"fake launcher")?;
        fs::write(&payload, b"payload")?;
        pack_files(&launcher, &output, &[payload.display().to_string()])?;

        let mut file = OpenOptions::new().write(true).open(&output)?;
        file.seek(SeekFrom::End(-17))?;
        file.write_all(&u64::MAX.to_le_bytes())?;

        assert_eq!(read_back(&output).unwrap_err().kind(), ErrorKind::InvalidData);
        Ok(())
    }
}
