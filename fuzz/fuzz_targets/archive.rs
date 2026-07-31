#![no_main]

use libfuzzer_sys::fuzz_target;
use spec_elf::archive::format::{is_archive, read_back};
use std::io::Write;

fuzz_target!(|data: &[u8]| {
    let Ok(mut file) = tempfile::NamedTempFile::new() else {
        return;
    };

    if file.write_all(data).is_err() {
        return;
    }

    if is_archive(file.path()).unwrap_or(false) {
        let _ = read_back(file.path());
    }
});
