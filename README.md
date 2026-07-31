<div align="center">
  <h1>spec-elf</h1>
  <p><strong>One executable. Five x86-64 builds. The best one selected automatically.</strong></p>
  <p>
    <a href="#quick-start">Quick start</a> ·
    <a href="#supported-projects">Supported projects</a> ·
    <a href="#how-it-works">How it works</a> ·
    <a href="docs/functions.md">Function cheat sheet</a> ·
    <a href="docs/format.md">File format</a>
  </p>
</div>

<hr>

<p>
  <code>spec-elf</code> packages multiple x86-64 builds of a project into a single Linux ELF or Windows PE executable.
  At runtime, it detects the host CPU and launches the best compatible payload. Linux specializes the packed file
  permanently on first run; Windows uses a temporary executable because a running <code>.exe</code> cannot replace itself.
</p>

<table>
  <tr>
    <th align="left">Platforms</th>
    <td>Linux and Windows x86-64</td>
    <th align="left">Languages</th>
    <td>C, C++, Rust, Zig</td>
  </tr>
  <tr>
    <th align="left">Variants</th>
    <td>native, baseline, v2, v3, v4</td>
    <th align="left">Compression</th>
    <td>Zstandard, per payload</td>
  </tr>
</table>

<h2 id="quick-start">Quick start</h2>

<h3>1. Build spec-elf</h3>

<pre><code class="language-bash">cargo build --release</code></pre>

<p>
  The launcher is written to <code>target/release/spec-elf</code> on Linux and
  <code>target/release/spec-elf.exe</code> on Windows.
</p>

<h3>2. Package a project</h3>

<p>Pass the project directory explicitly. Use <code>.</code> for the current directory.</p>

<pre><code class="language-bash">cd /path/to/project
/path/to/spec-elf/target/release/spec-elf .</code></pre>

<p>Or package it from elsewhere:</p>

<pre><code class="language-bash">/path/to/spec-elf/target/release/spec-elf /path/to/project</code></pre>

<p>On Windows PowerShell:</p>

<pre><code class="language-powershell">Set-Location C:\path\to\project
&amp; "C:\path\to\spec-elf\target\release\spec-elf.exe" .</code></pre>

<p>
  The resulting packed executable is named <code>spec-elf</code> on Linux or <code>spec-elf.exe</code> on Windows and is
  placed in the project directory. Intermediate binaries are written to that project's <code>build/</code> directory.
</p>

<blockquote>
  <strong>Host builds only:</strong> run <code>spec-elf</code> on Linux to package Linux executables and on Windows to
  package Windows executables. Cross-compiling a package for another operating system is not currently supported.
</blockquote>

<h2>CPU variants</h2>

<p>Every supported project is built for these targets:</p>

<table>
  <thead>
    <tr>
      <th align="left">Variant</th>
      <th align="left">When it is selected</th>
    </tr>
  </thead>
  <tbody>
    <tr><td><code>native</code></td><td>The CPU identity matches the machine that built the package.</td></tr>
    <tr><td><code>x86-64-v4</code></td><td>The host supports the complete v4 feature set.</td></tr>
    <tr><td><code>x86-64-v3</code></td><td>The host supports the complete v3 feature set.</td></tr>
    <tr><td><code>x86-64-v2</code></td><td>The host supports the complete v2 feature set.</td></tr>
    <tr><td><code>x86-64</code></td><td>Baseline fallback for any x86-64 host.</td></tr>
  </tbody>
</table>

<p>
  The launcher uses <code>native</code> only for a matching CPU. Otherwise, it selects the highest
  compatible standardized x86-64 level.
</p>

<h2 id="supported-projects">Supported projects</h2>

<p>
  <code>spec-elf</code> detects the project language by recursively counting source-file extensions.
  It ignores <code>target/</code>, <code>build/</code>, and <code>.git/</code> directories.
</p>

<table>
  <thead>
    <tr>
      <th align="left">Language</th>
      <th align="left">Detected by</th>
      <th align="left">Build behavior</th>
    </tr>
  </thead>
  <tbody>
    <tr>
      <td><strong>C</strong></td>
      <td><code>.c</code></td>
      <td>Uses CMake when <code>CMakeLists.txt</code> exists; otherwise uses <code>gcc -O3</code>.</td>
    </tr>
    <tr>
      <td><strong>C++</strong></td>
      <td><code>.cpp</code>, <code>.cc</code>, <code>.cxx</code>, <code>.hpp</code>, <code>.hxx</code></td>
      <td>Uses CMake when <code>CMakeLists.txt</code> exists; otherwise uses <code>g++ -O3</code>.</td>
    </tr>
    <tr>
      <td><strong>Rust</strong></td>
      <td><code>.rs</code></td>
      <td>Finds the nearest <code>Cargo.toml</code>, builds in release mode, and sets <code>RUSTFLAGS</code> per target.</td>
    </tr>
    <tr>
      <td><strong>Zig</strong></td>
      <td><code>.zig</code></td>
      <td>Builds the first Zig source with <code>zig build-exe -O ReleaseFast</code>.</td>
    </tr>
  </tbody>
</table>

<h3>Toolchain requirements</h3>

<ul>
  <li>Rust and Cargo to build <code>spec-elf</code>.</li>
  <li><code>gcc</code> or CMake with a GCC-compatible C compiler for C projects.</li>
  <li><code>g++</code> or CMake with a GCC-compatible C++ compiler for C++ projects.</li>
  <li>Cargo for Rust projects.</li>
  <li>Zig for Zig projects.</li>
</ul>

<h2 id="how-it-works">How it works</h2>

<ol>
  <li>Build five CPU-specific versions of the project.</li>
  <li>Compress each payload independently with Zstandard.</li>
  <li>Append the compressed frames and a manifest to the launcher.</li>
  <li>Detect the current CPU's x86-64 feature level at runtime.</li>
  <li>Extract the best matching payload to a temporary sibling file.</li>
  <li>On Linux, atomically replace the launcher and execute the selected payload.</li>
  <li>On Windows, run the temporary <code>.exe</code>, forward its exit code, and remove it afterward.</li>
</ol>

<p>
  Linux permanently replaces the packed file after specialization. Windows keeps the packed launcher because the
  operating system locks running executables. Both platforms require write access to the launcher's directory.
  Runtime arguments are forwarded to the selected program. See
  <a href="docs/format.md">the packed-format documentation</a> for the binary layout.
</p>

<details>
  <summary><strong>Current limitations</strong></summary>
  <br>
  <ul>
    <li>Only x86-64 Linux and Windows are supported.</li>
    <li>The packaging CLI accepts exactly one project-directory argument; packed programs may receive arbitrary runtime arguments.</li>
    <li>The packaged output is named after the launcher binary, normally <code>spec-elf</code> or <code>spec-elf.exe</code>.</li>
    <li>Packages are built for the host operating system; cross-OS packaging is not supported.</li>
    <li>Direct C and C++ builds cannot supply custom libraries, linker flags, or complex include paths; use CMake for those projects.</li>
    <li>C and C++ CPU variants currently require GCC-compatible <code>-march</code> flags; MSVC is not supported.</li>
    <li>CMake packaging expects exactly one executable in its configured runtime output directory.</li>
    <li>Rust supports the default binary or one explicit <code>[[bin]]</code>; multiple explicit binaries require a future selection option.</li>
    <li>Zig currently builds the first <code>.zig</code> source found.</li>
    <li>Each decompressed payload is limited to 1 GiB.</li>
    <li>On Windows, remove an existing packed output before rebuilding it; Windows does not allow the final rename to replace an existing file.</li>
  </ul>
</details>

<h2>Development</h2>

<table>
  <thead>
    <tr><th align="left">Task</th><th align="left">Command</th></tr>
  </thead>
  <tbody>
    <tr><td>Run tests</td><td><code>cargo test</code></td></tr>
    <tr><td>Show CLI help</td><td><code>cargo run -- --help</code></td></tr>
    <tr><td>Package a project</td><td><code>cargo run -- /path/to/project</code></td></tr>
    <tr><td>Fuzz the archive parser</td><td><code>cargo fuzz run archive</code> from <code>fuzz/</code></td></tr>
  </tbody>
</table>

<hr>

<p align="center">
  <strong>Experimental software.</strong><br>
  Useful for testing CPU-specialized builds; not yet a general-purpose application packager.
</p>
