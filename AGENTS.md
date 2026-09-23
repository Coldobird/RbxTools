# Release policy

- Every new application version must automatically create a GitHub Release when its first `main` build completes.
- The release tag is `v` followed by the version in `src-tauri/tauri.conf.json`.
- Ordinary commits that do not change to an unreleased version still build and upload an Actions artifact, but do not create another release.

# Visual direction

- Keep all new images, logos, and interface icons in the established GBA pixel-art style: square pixels, crisp edges, stepped curves, and the app's burgundy, red, and peach palette.
- Reuse the shared pixel icon set in `src/PixelIcons.tsx` for interface controls and statuses. Avoid mixing in smooth outline icons or photographic assets.
