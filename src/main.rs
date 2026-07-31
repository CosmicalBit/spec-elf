use spec_elf::archive::format::{is_archive, pack_files, read_back};
use spec_elf::builder::compile::compile_lang;
use std::{
    env,
    ffi::OsStr,
    fs,
    fs::OpenOptions,
    io::{ErrorKind, Write},
    path::{Path, PathBuf},
    process::Command,
};

#[cfg(unix)]
use std::os::unix::{fs::PermissionsExt, process::CommandExt};

fn help() -> ! {
    println!("Usage: spec-elf <project-directory>");
    println!("Use `.` for the current directory.");
    println!();
    println!(
        "Builds compatible x86-64 variants of a C, C++, Rust, or Zig project and packages them into one executable."
    );
    std::process::exit(0);
}

fn main() -> Result<(), anyhow::Error> {
    let current_path = env::current_exe()?;

    if is_archive(&current_path)? {
        return specialize_and_run(&current_path);
    }

    let args: Vec<_> = env::args_os().skip(1).collect();

    let project_dir = match args.as_slice() {
        [flag] if is_help_flag(flag) => help(),
        [directory] if !directory.is_empty() => directory,
        [] => anyhow::bail!("missing project directory; use `.` for the current directory"),
        _ => anyhow::bail!("invalid arguments; run `spec-elf --help` for usage"),
    };

    let current_name = current_path.file_name().expect("current executable has no file name");

    env::set_current_dir(project_dir).map_err(|error| {
        anyhow::anyhow!(
            "could not change to project directory `{}`: {error}",
            Path::new(project_dir).display()
        )
    })?;

    let dir = env::current_dir()?;
    let dst = compile_lang(&dir)?;

    let output_path = dir.join(current_name);

    #[cfg(windows)]
    let output_path = if same_path(&current_path, &output_path) {
        let stem = current_path.file_stem().and_then(OsStr::to_str).unwrap_or("spec-elf");
        dir.join(format!("{stem}-packed.exe"))
    } else {
        output_path
    };

    install_packed_output(&current_path, &output_path, &dst)?;
    println!("Packed executable: {}", output_path.display());

    Ok(())
}

fn install_packed_output(launcher: &Path, output: &Path, payloads: &[String]) -> Result<(), anyhow::Error> {
    let parent = output
        .parent()
        .ok_or_else(|| anyhow::anyhow!("output path has no parent directory"))?;

    for attempt in 0..100 {
        let temporary_path = parent.join(format!(".spec-elf-pack-{}-{attempt}.tmp", std::process::id()));

        match pack_files(launcher, &temporary_path, payloads) {
            Ok(()) => {}
            Err(error) if error.kind() == ErrorKind::AlreadyExists => continue,
            Err(error) => {
                let _ = fs::remove_file(&temporary_path);
                return Err(error.into());
            }
        }

        #[cfg(unix)]
        if let Err(error) = fs::set_permissions(&temporary_path, fs::Permissions::from_mode(0o755)) {
            let _ = fs::remove_file(&temporary_path);
            return Err(error.into());
        }

        if let Err(error) = fs::rename(&temporary_path, output) {
            let _ = fs::remove_file(&temporary_path);
            return Err(anyhow::anyhow!(
                "could not install packed executable at {}: {error}",
                output.display()
            ));
        }

        return Ok(());
    }

    anyhow::bail!("could not create a temporary package in {}", parent.display())
}

fn is_help_flag(value: &OsStr) -> bool {
    ["--help", "-help", "-h", "--h"].iter().any(|flag| value == *flag)
}

#[cfg(unix)]
fn specialize_and_run(current_path: &Path) -> Result<(), anyhow::Error> {
    let payload = read_back(current_path)?;
    let temporary_path = write_temporary_payload(current_path, &payload)?;

    if let Err(error) = fs::rename(&temporary_path, current_path) {
        let _ = fs::remove_file(&temporary_path);
        return Err(error.into());
    }

    let error = Command::new(current_path).args(env::args_os().skip(1)).exec();
    Err(error.into())
}

#[cfg(windows)]
fn specialize_and_run(current_path: &Path) -> Result<(), anyhow::Error> {
    let payload = read_back(current_path)?;
    let temporary_path = write_temporary_payload(current_path, &payload)?;

    let status = Command::new(&temporary_path).args(env::args_os().skip(1)).status();
    let cleanup_result = fs::remove_file(&temporary_path);
    let status = status?;

    if let Err(error) = cleanup_result
        && error.kind() != ErrorKind::NotFound
    {
        eprintln!("warning: could not remove {}: {error}", temporary_path.display());
    }

    std::process::exit(status.code().unwrap_or(1));
}

fn write_temporary_payload(current_path: &Path, payload: &[u8]) -> Result<PathBuf, anyhow::Error> {
    let parent = current_path
        .parent()
        .ok_or_else(|| anyhow::anyhow!("current executable has no parent directory"))?;
    let name = current_path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| anyhow::anyhow!("current executable has no valid file name"))?;

    for attempt in 0..100 {
        let temporary_path = parent.join(format!(
            ".{name}.spec-elf-{}-{attempt}{}",
            std::process::id(),
            env::consts::EXE_SUFFIX
        ));
        let file = OpenOptions::new().create_new(true).write(true).open(&temporary_path);

        let mut file = match file {
            Ok(file) => file,
            Err(error) if error.kind() == ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(error.into()),
        };

        let write_result = (|| -> std::io::Result<()> {
            file.write_all(payload)?;

            #[cfg(unix)]
            file.set_permissions(fs::Permissions::from_mode(0o755))?;

            file.sync_all()
        })();

        if let Err(error) = write_result {
            drop(file);
            let _ = fs::remove_file(&temporary_path);
            return Err(error.into());
        }

        return Ok(temporary_path);
    }

    anyhow::bail!(
        "could not create a temporary executable next to {}",
        current_path.display()
    )
}

#[cfg(windows)]
fn same_path(left: &Path, right: &Path) -> bool {
    match (left.canonicalize(), right.canonicalize()) {
        (Ok(left), Ok(right)) => left == right,
        _ => left == right,
    }
}
