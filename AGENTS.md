# Release policy

- Every new application version must automatically create a GitHub Release when its first `main` build completes.
- The release tag is `v` followed by the version in `src-tauri/tauri.conf.json`.
- Ordinary commits that do not change to an unreleased version still build and upload an Actions artifact, but do not create another release.
