# Development and release

## Prerequisites

- Rust stable, at least 1.85, with rustfmt and clippy. `Cargo.lock` pins the verified
  dependency set; binding/tool dependencies can require a newer stable compiler.
- A native Rust linker (MSVC C++ Build Tools on Windows).
- Python 3.10+ and a virtual environment for maturin.
- Node.js 22+ (ESLint 10 requires Node 22.13+ on the Node 22 line), npm, wasm-pack
  0.15.0, and the `wasm32-unknown-unknown` Rust target.

The root crate contains only the container and codec. `bindings/python` and
`bindings/wasm` depend on it. Add future codecs in the root crate, extend the
explicit algorithm dispatch, document the new tuple in `FORMAT.md` and retain
v1 fixture decoding. Binding layers must not implement different codecs.

## Local checks

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --locked
npm ci
npm run lintfix

python -m pip install "maturin>=1.9,<2"
maturin develop --release --locked
python -m unittest discover -s tests -p "test_python.py"

npm run build
npm test
npm run pack:check
```

After changing Cargo dependencies, run `cargo fetch --locked` followed by
`python scripts/collect_licenses.py` and commit the updated third-party notices.
Binary packages include those upstream license texts.

On Windows, use `npm.cmd` if PowerShell script execution policy blocks `npm`.
Python commands assume an activated `.venv`. No Cypress test suite is configured.
Node executes both the Node.js target and the web ESM target initialized with WASM
bytes; this does not establish behavior in an actual browser or a particular bundler.

Rust tests cover opaque byte recovery, empty and incompressible input, all presets,
version dispatch, CRC corruption, every truncation of a small archive and its inner
stream, trailing/concatenated streams, false sizes and decompression limits.
Python's standard liblzma-backed codec provides independent interoperability tests.
Committed fixtures are synthetic. Optional real replay measurements are separate
from CI and are not a statistical performance benchmark.

For local real-world measurements, add original replays under `tests/samples/`
and run `python scripts/check_samples.py` with the Python binding installed.
The [sample checklist](../tests/samples/README.md) describes useful formats and
edge cases. Samples stay outside Git and release packages; the generated CSV goes
to `target/sample-report.csv`. This is a one-run measurement, not a statistical
benchmark or replay-format validation.

## Build installable artifacts without accounts

```sh
python scripts/check_version.py
maturin build --release --locked --out dist/wheels
maturin sdist --out dist/sdist
npm run build
npm pack ./dist/npm --pack-destination dist
```

Install the wheel in a separate virtual environment, run the Python tests against
it, then install the `.tar.gz` source distribution in another clean environment to
verify the archive contains the Rust workspace dependencies. Building the source
distribution requires Rust and maturin (pip's build isolation downloads maturin).

To check the npm tarball, install it into a clean consumer directory and set
`MSRC_NPM_PACKAGE` to the absolute path of its `node_modules/@putianyi888/msrc` directory before
running `npm test`. CI does this so the tests run against packaged files.

The checked-in hex fixture is historical compatibility evidence: do not regenerate
it just because encoder output changes. `python tests/generate_fixture.py` explains
how it was originally constructed; compare decoded bytes when adding new fixtures.

## CI and publishing

The `CI and packages` GitHub workflow checks Rust on Linux, Windows and macOS;
builds Python wheels for Linux x86_64 (manylinux2014), Windows x64 and macOS arm64;
tests an independently built sdist on Linux; builds/tests/packs npm; and uploads
artifacts. Additional architectures can be added to the Python matrix later.
No publishing credentials are needed for these build jobs.

## Python: GitHub Releases

The repository is `https://github.com/putianyi889/MSRC`. Python distributions use
the name `msrc` and the import remains `import msrc`. Users install from a GitHub
Release asset URL or a downloaded wheel; there is no index-based installation.

After pushing the implementation and obtaining a successful CI run, create and
push a tag matching the package version:

```sh
git tag v0.1.0
git push origin v0.1.0
```

With GitHub CLI installed and authenticated, dispatch the workflow on that tag:

```sh
gh workflow run ci.yml --ref v0.1.0 -f publish_python=true -f publish_npm=false
```

The workflow builds and tests all packages, downloads the Python artifacts from
that same run, generates `SHA256SUMS` and creates a GitHub Release containing all
wheels, the source `.tar.gz` and the checksum manifest. It uses the automatic
`GITHUB_TOKEN` via `github.token`, scoped to `contents: write` in the publishing
job. No separate Python package registry account, token or environment is required.
The [official `gh release create` command](https://cli.github.com/manual/gh_release_create)
verifies the remote tag with `--verify-tag` and uploads the provided files.
An existing release is not overwritten; publish a new version for changed files.

To install the Windows x64 wheel (CPython 3.10+), after that release exists:

```sh
python -m pip install "https://github.com/putianyi889/MSRC/releases/download/v0.1.0/msrc-0.1.0-cp310-abi3-win_amd64.whl"
```

For Linux or macOS, choose the wheel matching your platform from the Release page.
The wheel includes the native Rust library, so installing it does not require a
Rust compiler. To build directly from the version tag instead, install Git, Rust
and a native linker first:

```sh
python -m pip install "git+https://github.com/putianyi889/MSRC.git@v0.1.0"
```

This uses [pip's supported VCS installation syntax](https://pip.pypa.io/en/stable/topics/vcs-support/).

## JavaScript: npm

The npm package name is `@putianyi888/msrc`, published with public access. The
generated manifest takes its name and `publishConfig` from the root `package.json`;
`publishConfig.access` is `public` and the registry is `https://registry.npmjs.org/`.
The root's `private: true` applies only to the development workspace and is not
copied into the distributable manifest. The generated manifest includes the GitHub
repository URL for provenance.

Version `0.1.0` was published manually. Subsequent releases use
[npm Trusted Publishing](https://docs.npmjs.com/trusted-publishers/) with GitHub
Actions OIDC. The publish job runs on a GitHub-hosted Ubuntu runner with Node 24,
npm 11, and job-scoped `id-token: write`. It does not receive a stored npm token or
configure token authentication. The Python package remains `msrc`.

Push the workflow to GitHub, create the repository environment `npm`, and configure
the package's npm settings under **Trusted Publisher → GitHub Actions**:

| Field | Value |
| --- | --- |
| Organization or user | `putianyi889` (GitHub owner, not the npm scope) |
| Repository | `MSRC` |
| Workflow filename | `ci.yml` |
| Environment name | `npm` |
| Allowed actions | Enable direct publishing with `npm publish` |

The workflow filename is just `ci.yml`, not `.github/workflows/ci.yml` or its
display name. The repository URL in the package metadata matches this repository.
The npm scope remains `putianyi888`.

According to npm's current policy, a newly created trusted publisher must complete
its first successful publish within two days, so configure it when the next release
is ready. Creating the configuration does not verify it; its first real publish is
the end-to-end authentication check.

The already published `0.1.0` cannot be uploaded again. For the next npm release,
first bump the Rust workspace and root npm versions together (for example to
`0.1.1`), refresh the lockfiles, commit/push, and create the matching tag. Then run:

```sh
gh workflow run ci.yml --ref v0.1.1 -f publish_python=false -f publish_npm=true
```

Set both options to `true` to publish both channels. Publishing depends on all
build/check jobs in that same run. Pushes, PRs and tags alone only build artifacts;
both publication options default to `false`.

For later versions, update `[workspace.package]` and `package.json` together,
refresh lockfiles, review checks and artifacts, then create the matching new tag.
The npm `0.1.0` release was published manually; no automated trusted publication
has yet been verified. Python GitHub Release publication is configured separately.

`scripts/check_version.py` checks Rust/npm version consistency and any current tag.
Python takes its version from the Rust workspace. Crates.io publication is optional
and has no automatic publishing job; the Rust core can already be used by path.
