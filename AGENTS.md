# Release policy

- The user's word “commit” always includes committing and pushing to `origin/main`; bump the application patch version unless they specify another version.
- Every commit request must run `npm run test:ui` before committing. Fix any failures and rerun until it passes; do not weaken the alignment assertions or tolerance.
- Every new application version must automatically create a GitHub Release when its first `main` build completes.
- The release tag is `v` followed by the version in `src-tauri/tauri.conf.json`.
- Ordinary commits that do not change to an unreleased version still build and upload an Actions artifact, but do not create another release.
- Whenever the application version is bumped, build it locally and publish the matching executable, NSIS installer, and `latest.json` to the shared OneDrive Builds folder for shared local distribution. The in-app updater uses the portable executable and SHA-256 digest from GitHub Releases. Verify that the OneDrive manifest names the new version and installer. If OneDrive is unavailable, report that the publish is incomplete.

# Commit delegation

- For explicit user requests to commit, delegate the complete release policy above to a GPT-6 Luna sub-agent (`gpt-6-luna`) with high reasoning, using a compact handoff. The sub-agent owns the version bump, checks, local build, OneDrive publish and manifest verification, commit, push, and GitHub Release verification; the main chat keeps its current model and reports the result.
- Respect any model override the user specifies. The commit sub-agent must not delegate or recursively hand off the commit work.
- Delegate only for explicit commit requests. If delegation is unavailable, disclose that limitation rather than silently changing the main chat's model.

# Visual direction

- Keep all new images, logos, and interface icons in the established GBA pixel-art style: square pixels, crisp edges, stepped curves, and the app's burgundy, red, and peach palette.
- Reuse the shared pixel icon set in `src/PixelIcons.tsx` for interface controls and statuses. Avoid mixing in smooth outline icons or photographic assets.

# UI test view

- Activate or exit the installed app's UI test view with **Ctrl+Shift+T**. In a browser preview, append **`?ui-test=1`** to the app URL (for example `http://127.0.0.1:1420/?ui-test=1`).
- Use this view for future UI checks. Its controls preview both pages, all Steam states, success/error/update toasts and stacks, Settings/startup-update/force-restart modals, busy states, and update errors. Hide controls for unobstructed screenshots.
- The view uses the actual app components with simulated actions; Steam, network, privacy, file dialogs, and installation commands are disabled. Toasts remain visible for inspection. Exit returns to normal operation; reopening a URL with `?ui-test=1` reactivates test mode.

# UI alignment checks

- After changing typography, icons, badges, or layout CSS, run `npm run test:ui` in addition to the relevant frontend checks.
- These browser checks measure the visible pixels of icons and text in the UI test view. Centering an element's CSS box alone is insufficient.
- Keep the alignment tolerance at one CSS pixel. Fix the UI when a check fails; do not widen the tolerance to accept a regression.

# Long-running test monitoring

- Delegate monitoring of long-running tests and processes to a GPT-6 Luna sub-agent (`gpt-6-luna`) with high reasoning, using a compact handoff with the process, expected duration, evidence paths, and cleanup requirements.
- After monitoring completes, continue the main work and report the results with GPT-6.1 Sol (`gpt-6.1-sol`). When the main chat already uses GPT-6.1 Sol, keep that model while Luna handles monitoring.
- Respect any later model override from the user. If the requested delegation is unavailable, disclose the limitation.
