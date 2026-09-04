# Publish release packages

The [release workflow](../.github/workflows/release.yml) builds Windows x64,
Linux AMD64, and Linux ARM64 on native GitHub-hosted runners. Windows uses
`windows-2022`; Linux uses `ubuntu-24.04` and `ubuntu-24.04-arm`.

1. Update the version in `crates/skyroads-sdl/Cargo.toml` and refresh
   `Cargo.lock` with Cargo. Release tags must match that version exactly.
2. Commit and push the release changes. For an unpublished trial, choose
   **Actions → Release packages → Run workflow** on the desired branch.
   Pull requests also build and test the packages. These runs upload Actions
   artifacts for 14 days and do not create a GitHub release.
3. Push a version tag on the chosen commit, for example `v0.1.0` or
   `v0.2.0-beta.1` when the Cargo version is `0.2.0-beta.1`.
4. Check that all three package jobs and the publish job finish successfully.
   Only a tag push publishes; manually running the workflow on a tag still
   produces test artifacts only.

The workflow uses Rust 1.97.1 and locked dependencies. Windows uses the official
SDL 2.32.10 VC archive with a pinned SHA-256 and includes its x64 DLL and license.
Linux dynamically links the distribution's SDL2; users install `libsdl2-2.0-0`.
GitHub's built-in token receives `contents: write` only in the publish job.
No personal token or separately configured secret is required.

All architectures must pass workspace tests, archive checks, and packaged
gameplay/procedural/gamepad smoke tests before publication. The publish job
checks the tag's commit and archive checksums, uploads assets to a draft, then
makes the complete release visible. Versions containing a hyphen become
prereleases. Existing releases are not overwritten. If publication fails after
draft creation, inspect or delete that incomplete draft before rerunning the
publish job; do not move a published tag to another commit.

Each release contains:

| File | Platform |
| --- | --- |
| `SkyRoads-Rust-VERSION-windows-x64.zip` | Windows AMD64/x86-64 |
| `SkyRoads-Rust-VERSION-linux-x64.tar.gz` | Linux AMD64/x86-64 |
| `SkyRoads-Rust-VERSION-linux-arm64.tar.gz` | Linux ARM64/AArch64 |
| `SHA256SUMS.txt` | SHA-256 of all three archives |

These are portable archives, not installers. Linux compatibility starts at
Ubuntu 24.04 or a compatible system with glibc and SDL2. ARM means ARM64;
32-bit ARM and Windows ARM64 are not build targets.

## Original game redistribution

Research checked on 2026-09-04: the developer's
[full original download](http://www.bluemoon.ee/history/skyroads/skyroads.zip)
contains `readme.txt` with the same terms as this
[Bluemoon freeware license copy](https://www.classicdosgames.com/files/licenses/Bluemoon_Interactive_Freeware_License.txt).
Redistribution requires the complete original program and accompanying files
to stay intact and unmodified; the terms also restrict reverse engineering.
The [developer's page](http://www.bluemoon.ee/history/skyroads/index.html)
offers the full game but supplies no broader asset-reuse permission.

These terms do not clearly permit shipping a selected asset bundle with this
reverse-engineered native port. Consequently release packages exclude the DOS
executable and all original data, including graphics, levels, audio, and demo
recordings. [GAME-DATA.txt](GAME-DATA.txt) tells users how to supply their copy.
Procedural mode also needs those assets. This decision is not a legal clearance
of the port or its compiled, recovered runtime tables; those remain a separate
rights question.

`release.py` constructs archives from an explicit payload list instead of
copying the workspace. Smoke tests use a separate temporary data directory,
so game data and generated settings cannot enter the archive. The build emits
dependency license texts in `THIRD-PARTY-NOTICES.txt` and, on Windows, the SDL
license. It does not assign a new license to the project or original game.

The current source tree no longer tracks the original root game files;
`.gitignore` prevents accidentally adding local test copies. Their removal does
not rewrite earlier Git history. `.gitattributes` also excludes original game
files and captured DOS fixtures from `git archive`/GitHub-generated source
archives. Source archives can build the native port without the game data.

CI builds the native executable first, then downloads the developer's complete
original archive and verifies its pinned SHA-256 before extracting it into the
temporary runner checkout. The existing fixture tests require these root data
files, including `SKYROADS.EXE`. If the download or checksum check fails, the
workflow fails instead of skipping tests. This data is never included in the
uploaded packages.

## Local package verification

From a Linux checkout with SDL2 development files and Rust installed, first
extract your full original game into the checkout root for the fixture tests.
Git ignores these local copies. Then run:

```bash
cargo fetch --locked
cargo test --locked --workspace
cargo build --locked --release -p skyroads-sdl
python3 -m unittest discover -s packaging -p 'test_*.py'
python3 packaging/release.py --platform linux-x64 --binary target/release/skyroads-sdl
```

Use `linux-arm64` on an ARM64 host. On native Windows, build with the workflow's
SDL settings, then pass `--platform windows-x64`, the `.exe` path, and
`--sdl-directory` pointing to the extracted `SDL2-2.32.10` VC directory.
The packager requires Python 3.10+, fetched Cargo dependency sources, and the
original data for its temporary smoke tests. It writes archives and individual
checksums to `target/packages/`, preserving existing `dist/` packages.

## Package documentation sources

`README-WINDOWS.txt` and `README-LINUX.txt` are the canonical platform notes
for generated release archives. Package assembly must copy the matching file
without renaming it and must also include the repository `README.md`.

Each archive includes `BUILD-INFO.json` with its version, target, source commit,
and compiler. The workflow log records the automated acceptance of that build.
Platform notes retain dated physical-controller evidence; automated runs must
not turn historical evidence into a certification of each new build. When
physical acceptance is performed, update the platform note with its
operating-system version, SDL runtime,
connection mode, controller firmware, and mapped/raw result. The current SN30
Pro Bluetooth evidence is partial and does not complete an acceptance row. Do
not replace that boundary with a support claim until the controller hardware
matrix in [`plans/expand-controller-support.md`](../plans/expand-controller-support.md)
has passed.

Do not add a controller mapping file speculatively. Include one only after a
physical-device failure proves that the packaged SDL mappings are insufficient,
and record the mapping source and license alongside it.
