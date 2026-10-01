# GitHub Actions build measurements

Measured on September 30, 2026 using GitHub-hosted Windows runners and Rust 1.98.1. The baseline and optimized commits contain identical application source; only `.github/workflows/build-release.yml` differs. Both produce the executable and NSIS installer and pass frontend and native tests. Release publishing was disabled for every benchmark.

| Measurement | Previous workflow | Optimized workflow, cached dependencies | Reduction |
| --- | ---: | ---: | ---: |
| Complete job | 10m55s | 6m38s | 39.2% (4m17s saved) |
| App and installer build step | 5m44s | 4m22s | 23.8% |
| Rust cache restore | 1m53s | 35s | 69.0% |
| Native test build and execution | 1m50s | 15s | 86.4% |
| Compressed Rust cache | 1,379 MiB | 390 MiB | 71.7% |

The first optimized run, with no existing smart Rust cache, completed in 6m13s. It created the 390 MiB cache in a 31-second post step. Its native tests took 9 seconds. Runner performance varies, so the first run happened to finish faster than the subsequent cached run; these measurements are observations, not guaranteed durations. Job times include setup and post steps and exclude time waiting for a runner.

## Changes

- Use `Swatinem/rust-cache` to key dependencies by compiler and Cargo configuration, and remove workspace outputs and incremental artifacts from the cache. The old cache key considered only `Cargo.lock`.
- Test only the native library with the release profile and `tauri/custom-protocol`, after Tauri builds the frontend and executable. This reuses the production dependencies and avoids the separate debug build and compilation of diagnostic examples.
- Generate icons once through `desktop:build` and its input cache.
- Install npm dependencies with `--no-audit --no-fund` and use compression level 1 for the small Actions artifact archive. The executable and installer retain their normal production optimization and packaging settings.
- Add `benchmark_only` to manual workflow inputs so measurements can run without publishing a release. Normal main builds retain automatic releases for new application versions and artifact uploads for ordinary commits.

The baseline reported a Rust cache hit but still recompiled release dependencies. The optimized follow-up restored its new cache and compiled only `rbx-tools` during release packaging. All runs passed seven frontend unit tests, two build-cache integration tests, and four native unit tests.

## Run evidence

- [Same-source baseline: run 16](https://github.com/Coldobird/RbxTools/actions/runs/36681281062), commit `69305b569b9bc0c69031b9e38294e72b7df9cd4e`.
- [Optimized first run: run 17](https://github.com/Coldobird/RbxTools/actions/runs/36681283422).
- [Optimized cached run: run 18](https://github.com/Coldobird/RbxTools/actions/runs/36682033263), commit `ad53f33702f91931f09811999872142e40407ce9`.
- [Previous production release: run 15](https://github.com/Coldobird/RbxTools/actions/runs/36081109964) took 10m28s, including 92 seconds restoring Rust cache and 235 seconds saving it. This is historical context; the table uses the new same-source baseline instead.
- [Rust cache action documentation](https://github.com/Swatinem/rust-cache) describes keying and cache cleanup.

The benchmark branches were temporary and the measurements preceded the normal commit and release process. The benchmarks did not update `main`, the application version, UI, GitHub Releases, or the OneDrive updater manifest.
