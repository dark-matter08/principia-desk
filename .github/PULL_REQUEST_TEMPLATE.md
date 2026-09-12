## What changed

<!-- One paragraph: what the desk does differently after this, and why. -->

## How it was verified

- [ ] `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, `cargo test --all-targets` (in `src-tauri/`)
- [ ] `npm run check` and `npm test`
- [ ] Tried in the desk (`npm run tauri dev`) or the preview (`npm run dev`), with a screenshot for anything visible

## Safety

- [ ] This change does not weaken any way out of a lock (phrase, recovery console, release token, dead man's switch), or it explains here why the change is safe.
- [ ] No committed migration was edited; a schema change is a new numbered migration and STORAGE.md says what it does.

## Notes for the reviewer

<!-- Anything you were unsure about, or a decision a reviewer should check. -->
