# Packed executable format

`spec-elf` appends its payloads, a manifest, and a fixed-size footer to a normal Linux ELF or Windows PE
launcher executable.

```text
[launcher executable]
[payload bytes]
[manifest]
[footer]
```

The launcher remains at the beginning, so the operating system can execute the packed file normally. All integer fields
are little-endian.

## Manifest

```text
u32 entry_count

repeated entry_count times:
    u32 name_length
    u8[name_length] name_utf8
    u64 payload_offset
    u64 payload_size
```

Payload offsets are absolute from the beginning of the packed file. Payload ranges must end before the manifest begins.
Payload names are UTF-8 and limited to 4096 bytes.

## Footer

The footer is the final 33 bytes:

```text
u8[8] magic             "VPKFOOT\0"
u64   manifest_offset
u64   manifest_size
u64   native_cpu_hash
u8    launch_flag       1
```

The final byte is a format marker rather than mutable launch state; it must always be <code>1</code>. The manifest must
end exactly where the footer begins. The reader also rejects overflowing ranges, impossible entry counts, oversized
names, trailing manifest data, and payloads that overlap the manifest.

## Payload selection

The `native` payload is used only when the stored CPU hash matches the current host identity. The hash includes the
operating system, architecture, CPU identity, relevant feature bits, and enabled extended-register state. A zero hash
means no native identity was available.

Otherwise, the launcher selects the highest compatible x86-64 level available, falling back through v4, v3, v2, and
baseline x86-64. Payload matching accepts hyphenated or underscored target names and ignores a final `.exe` extension.
For example, both `rust-x86_64_v3.exe` and `c-x86-64-v3` identify a v3 payload.

## Installation and execution

Packages are built for the host operating system; the format can contain either an ELF launcher with ELF payloads or a
PE launcher with PE payloads, but it does not make payloads portable between operating systems.

The packer writes a new temporary archive and renames it into place only after the archive is complete. On Linux, the
launcher performs the same pattern during first-run specialization and then replaces its process with the selected
payload. On Windows, the packed launcher cannot replace itself while running, so it writes a temporary sibling `.exe`,
waits for it to finish, forwards its exit code, and removes it. Both paths require write access to the launcher's
directory.

The format is experimental and may change between releases.
