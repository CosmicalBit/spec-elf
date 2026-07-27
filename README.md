<div align="center">
  <h1>⚙️ spec-elf</h1>
  <p><strong>One ELF, optimized for every x86-64 CPU level.</strong></p>
  <p>
    <img alt="Rust 2024" src="https://img.shields.io/badge/Rust-2024-DEA584?logo=rust&amp;logoColor=white">
    <img alt="Linux x86-64" src="https://img.shields.io/badge/platform-Linux%20x86--64-FCC624?logo=linux&amp;logoColor=black">
    <img alt="C, C++, Rust, Zig" src="https://img.shields.io/badge/projects-C%20%7C%20C%2B%2B%20%7C%20Rust%20%7C%20Zig-2F81F7">
    <img alt="Status: experimental" src="https://img.shields.io/badge/status-experimental-F0A202">
  </p>
  <p>Build five optimized variants of a project, package them behind a small launcher, and select the best compatible payload on first run.</p>
</div>

<hr>

<h2>What it produces</h2>

<p><code>spec-elf</code> detects the project language, builds CPU-specific executables, and appends them to one Linux ELF with a compact manifest and footer.</p>

<table>
  <thead>
    <tr>
      <th align="left">Variant</th>
      <th align="left">Selection rule</th>
    </tr>
  </thead>
  <tbody>
    <tr>
      <td><code>native</code></td>
      <td>Chosen only when the runtime CPU fingerprint matches the machine used for the native build.</td>
    </tr>
    <tr>
      <td><code>x86-64-v4</code></td>
      <td>Selected for hosts that expose the v4 feature level.</td>
    </tr>
    <tr>
      <td><code>x86-64-v3</code></td>
      <td>Selected for v3 hosts when no matching native payload is available.</td>
    </tr>
    <tr>
      <td><code>x86-64-v2</code></td>
      <td>Selected for v2 hosts when higher levels are unavailable.</td>
    </tr>
    <tr>
      <td><code>x86-64</code></td>
      <td>Generic baseline fallback.</td>
    </tr>
  </tbody>
</table>

<p>On first launch, the packed executable extracts the chosen payload, writes it under the launcher's filename in the current working directory, marks it executable, and starts it. The replacement payload is then a normal executable, so CPU detection is not repeated on later runs.</p>

<blockquote>
  <p><strong>Current path behavior:</strong> extraction targets the current working directory, not necessarily the packed executable's own directory. Run the packed file from its output directory while the project is experimental.</p>
</blockquote>

<h2>Supported project types</h2>

<table>
  <thead>
    <tr>
      <th align="left">Language</th>
      <th align="left">Build path</th>
      <th align="left">Required tool</th>
    </tr>
  </thead>
  <tbody>
    <tr>
      <td>C</td>
      <td>CMake when <code>CMakeLists.txt</code> exists; otherwise recursive source collection passed to <code>gcc</code>.</td>
      <td><code>gcc</code>, optionally <code>cmake</code></td>
    </tr>
    <tr>
      <td>C++</td>
      <td>CMake when available; otherwise recursive <code>.cpp</code>, <code>.cc</code>, and <code>.cxx</code> collection passed to <code>g++</code>.</td>
      <td><code>g++</code>, optionally <code>cmake</code></td>
    </tr>
    <tr>
      <td>Rust</td>
      <td>Five release builds with isolated target directories and per-variant <code>RUSTFLAGS</code>.</td>
      <td><code>cargo</code> and <code>rustc</code></td>
    </tr>
    <tr>
      <td>Zig</td>
      <td>The first discovered <code>.zig</code> source is built five times with <code>ReleaseFast</code>.</td>
      <td><code>zig</code></td>
    </tr>
  </tbody>
</table>

<p>Language detection counts recognized source extensions recursively and chooses the most common language. The <code>target</code>, <code>build</code>, and <code>.git</code> directories are ignored.</p>

<h2>Build spec-elf</h2>

<pre><code>cargo build --release</code></pre>

<p>The launcher is created at <code>target/release/spec-elf</code>.</p>

<h2>Package a project</h2>

<p>Pass a project directory explicitly. Use <code>.</code> for the current directory:</p>

<pre><code>cd /path/to/project
/path/to/spec-elf/target/release/spec-elf .</code></pre>

<p>Or package another directory:</p>

<pre><code>/path/to/spec-elf/target/release/spec-elf /path/to/project</code></pre>

<p>Intermediate variants are written below <code>build/</code>. The packed executable is named after the launcher—normally <code>spec-elf</code>—inside the target project directory.</p>

<h2>Packed layout</h2>

<table>
  <tbody>
    <tr>
      <td>1</td>
      <td>Launcher ELF</td>
    </tr>
    <tr>
      <td>2</td>
      <td>Five payload binaries</td>
    </tr>
    <tr>
      <td>3</td>
      <td>Manifest with each payload name, byte offset, and size</td>
    </tr>
    <tr>
      <td>4</td>
      <td>Footer containing <code>VPKFOOT\0</code>, manifest metadata, native CPU hash, and launch flag</td>
    </tr>
  </tbody>
</table>

<h2>Development</h2>

<pre><code>cargo fmt --all --check
cargo test
cargo clippy --all-targets
cargo run -- --help</code></pre>

<h2>Experimental limitations</h2>

<ul>
  <li>Linux and x86-64 only.</li>
  <li>Packaging creates build artifacts and replaces the target output filename; test in a disposable project first.</li>
  <li>Rust package-name discovery currently reads the first matching <code>name = "..."</code> line instead of fully parsing Cargo metadata.</li>
  <li>Zig support builds only the first discovered source file and does not use <code>build.zig</code>.</li>
  <li>CMake projects must produce exactly one executable in the configured runtime output directory.</li>
</ul>

<div align="center">
  <sub>Build once. Dispatch once. Run native.</sub>
</div>
