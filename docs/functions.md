# Function cheat sheet

Quick reference for the main functions in `spec-elf`. Functions marked **public** are exported by the library; the rest
are implementation details and may change without notice.

## Program flow

```text
main
├─ packed launcher? ── yes ──> specialize_and_run
│                              ├─ read_back
│                              ├─ write_temporary_payload
│                              └─ run selected payload
└─ project directory ──> compile_lang
                         ├─ compile_c / compile_cpp
                         ├─ compile_rust
                         └─ compile_zig
                              ↓
                       install_packed_output
                              ↓
                          pack_files
```

## `src/main.rs` — CLI and runtime

| Function | What it does |
| --- | --- |
| `main()` | Chooses between packaging mode and packed-launcher mode. In packaging mode it validates the CLI, builds all payloads, and installs the packed output. |
| `help()` | Prints CLI usage and exits successfully. Its `!` return type means it never returns. |
| `is_help_flag(value)` | Recognizes `--help`, `-help`, `-h`, and `--h` without requiring UTF-8 arguments. |
| `install_packed_output(launcher, output, payloads)` | Writes a package under a unique temporary name, applies Unix executable permissions, and renames it into place only after packing succeeds. |
| `specialize_and_run(current_path)` — Unix | Extracts the selected payload, atomically replaces the packed launcher, forwards runtime arguments, and replaces the current process with the payload. |
| `specialize_and_run(current_path)` — Windows | Extracts the selected payload to a temporary `.exe`, forwards runtime arguments, waits for it, removes it, and exits with the same status code. |
| `write_temporary_payload(current_path, payload)` | Securely creates a unique sibling file with `create_new`, writes and flushes the payload, and makes it executable on Unix. |
| `same_path(left, right)` — Windows | Compares canonical paths when possible. It prevents a running Windows launcher from being selected as its own output path. |

## `src/builder/compile.rs` — project detection and builds

| Function | Visibility | What it does |
| --- | --- | --- |
| `compile_lang(path)` | **Public** | Detects the project language and dispatches to the matching builder. Returns the five generated payload paths. |
| `compile_rust(path)` | **Public** | Builds five release variants with target-specific `RUSTFLAGS` and isolated Cargo target directories. |
| `detect_language(path)` | Internal | Recursively counts recognized extensions. It rejects empty projects and ties instead of guessing. |
| `count_languages_recursive(dir, counts)` | Internal | Walks the project tree while skipping `.git`, `build`, and `target`. |
| `compile_c(path)` | Internal | Builds five C variants with CMake or a direct `gcc` command. |
| `compile_cpp(path)` | Internal | Builds five C++ variants with CMake or a direct `g++` command. |
| `compile_zig(path)` | Internal | Builds the first sorted Zig source into five `ReleaseFast` variants. |
| `project_dir_from_path(path)` | Internal | Returns the directory itself, or the parent when the input is a file. |
| `find_cargo_project_dir(path)` | Internal | Walks upward until it finds the nearest `Cargo.toml`. |
| `cargo_binary_name(project_dir)` | Internal | Parses `Cargo.toml`, preferring one explicit `[[bin]]` name and otherwise using the package name. Rejects multiple explicit binaries. |
| `collect_sources(project_dir, extensions)` | Internal | Returns a stable, sorted list of matching source paths. |
| `collect_sources_recursive(dir, extensions, sources)` | Internal | Implements recursive source discovery while ignoring generated and repository directories. |
| `find_single_executable(dir)` | Internal | Finds the single CMake output executable; rejects zero or multiple candidates. Uses execute bits on Unix and `.exe` on Windows. |
| `find_first_source(project_dir, extensions)` | Internal | Returns the first item from the sorted source list. |

### Build target constants

| Constant | Used by | Values represented |
| --- | --- | --- |
| `MARCH_FLAGS` | C and C++ | `native`, `x86-64`, `x86-64-v2`, `x86-64-v3`, `x86-64-v4` |
| `ZIG_MARCH_FLAGS` | Zig | Equivalent Zig `-mcpu` values |
| `RUST_MARCH_FLAGS` | Rust | Equivalent Rust `-C target-cpu` values |

## `src/archive/format.rs` — package format

| Function | Visibility | What it does |
| --- | --- | --- |
| `pack_files(launcher_path, output_path, payload_paths)` | **Public** | Creates a new archive, copies the launcher and payloads, then appends the manifest and footer. Refuses to overwrite an existing output. |
| `read_back(path)` | **Public** | Validates the footer and manifest, selects the best compatible payload, and returns its bytes. |
| `is_archive(path)` | **Public** | Checks the fixed footer magic and marker without parsing or extracting the complete archive. |
| `read_u32(file)` | Internal | Reads one little-endian `u32` from the current file position. |
| `read_u64(file)` | Internal | Reads one little-endian `u64` from the current file position. |
| `ensure_available(file, end, size)` | Internal | Verifies that a manifest read stays inside the declared manifest range and cannot overflow. |
| `find_optimal(entries, native_hash)` | Internal | Supplies the current CPU level and native identity to the deterministic selection function. |
| `select_optimal(entries, native_hash, level, current_native_hash)` | Internal | Selects matching `native` first, then falls back from the highest supported standardized level to baseline. |
| `payload_label(name)` | Internal | Removes a final case-insensitive `.exe` extension before payload-name matching. |
| `label_matches(label, target)` | Internal | Matches a target as the complete label or as a suffix separated by `-` or `_`. |

## `src/arch/x86.rs` — CPU detection

| Function | Visibility | What it does |
| --- | --- | --- |
| `detect_x86_level()` | **Public** | Uses CPUID plus enabled OS register state to classify the host as baseline x86-64, v2, v3, or v4. |
| `native_hasher()` | **Public** | Hashes the host OS, architecture, CPU identity, feature bits, and extended-register state for safe `native` matching. Returns `None` when required CPU identity data is unavailable. |
| `push_feature(parts, name, enabled)` | Internal | Adds an enabled CPU feature to the normalized input used by `native_hasher`. |

### `X86Level`

The public `X86Level` enum is ordered from least to most capable:

```text
X86_64 < V2 < V3 < V4
```

That ordering lets payload selection reject targets above the detected host level and choose the highest available
compatible fallback.

## Tests

| Test | Protects against |
| --- | --- |
| `normal_file_is_not_archive` | Treating ordinary files as packed launchers. |
| `packed_file_is_archive` | Failing to recognize a valid packed launcher. |
| `packed_file_reads_best_payload` | Extracting the wrong CPU variant. |
| `selection_falls_back_to_a_lower_level` | Failing when the exact highest variant is absent. |
| `selection_accepts_windows_executable_names` | Mishandling `.exe` suffixes or mixed `-`/`_` target names. |
| `malformed_manifest_range_is_rejected` | Integer overflow and invalid manifest ranges. |
| `language_detection_ignores_c_headers` | Misclassifying C++ projects because of shared C headers. |
| `language_detection_rejects_ties` | Silently choosing one language when source counts tie. |
| `reads_explicit_cargo_binary_name` | Copying the wrong Rust executable when `[[bin]]` differs from the package name. |
