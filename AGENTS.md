# Release policy

- When the user asks to commit, bump the application patch version (unless they specify another version), then commit and push to `origin/main`.
- Every new application version must automatically create a GitHub Release when its first `main` build completes.
- The release tag is `v` followed by the version in `src-tauri/tauri.conf.json`.
- Ordinary commits that do not change to an unreleased version still build and upload an Actions artifact, but do not create another release.
- Whenever the application version is bumped, build it locally and publish the matching executable, NSIS installer, and `latest.json` to the shared OneDrive Builds folder so the in-app updater can see it. Verify that the manifest names the new version and installer. If OneDrive is unavailable, report that the publish is incomplete.

# Commit delegation

- For explicit user requests to commit, delegate the complete release policy above to a GPT-6 Luna sub-agent using a compact handoff. The sub-agent owns the version bump, checks, local build, OneDrive publish and manifest verification, commit, push, and GitHub Release verification; the main chat keeps its current model and reports the result.
- Respect any model override the user specifies. The commit sub-agent must not delegate or recursively hand off the commit work.
- Delegate only for explicit commit requests. If delegation is unavailable, disclose that limitation rather than silently changing the main chat's model.

# Visual direction

- Keep all new images, logos, and interface icons in the established GBA pixel-art style: square pixels, crisp edges, stepped curves, and the app's burgundy, red, and peach palette.
- Reuse the shared pixel icon set in `src/PixelIcons.tsx` for interface controls and statuses. Avoid mixing in smooth outline icons or photographic assets.
