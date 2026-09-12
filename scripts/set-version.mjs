// Set the app version everywhere it is written, so a release tag can match
// it: package.json, package-lock.json, src-tauri/tauri.conf.json,
// src-tauri/Cargo.toml and the crate's own entry in src-tauri/Cargo.lock.
// Usage: node scripts/set-version.mjs 0.2.0
import { readFile, writeFile } from 'node:fs/promises';

const version = process.argv[2];
if (!/^\d+\.\d+\.\d+(-[0-9A-Za-z.-]+)?$/.test(version ?? '')) {
  console.error('usage: node scripts/set-version.mjs <x.y.z>');
  process.exit(1);
}
const edit = async (path, transform) => {
  const before = await readFile(path, 'utf8');
  const after = transform(before);
  if (after === before) throw new Error(`${path}: nothing changed; is the version already there in the shape expected?`);
  await writeFile(path, after);
  console.log(`${path} → ${version}`);
};
await edit('package.json', (s) => s.replace(/("version":\s*")[^"]+(")/, `$1${version}$2`));
await edit('package-lock.json', (s) => {
  // The root entry and the "" package entry carry the version; dependencies keep theirs.
  let count = 0;
  return s.replace(/("name":\s*"principia-desk",\s*\n\s*"version":\s*")[^"]+(")/g, (_, a, b) => { count += 1; return `${a}${version}${b}`; });
});
await edit('src-tauri/tauri.conf.json', (s) => s.replace(/("version":\s*")[^"]+(")/, `$1${version}$2`));
await edit('src-tauri/Cargo.toml', (s) => s.replace(/^version = "[^"]+"/m, `version = "${version}"`));
await edit('src-tauri/Cargo.lock', (s) => s.replace(/(name = "principia-desk"\nversion = ")[^"]+(")/, `$1${version}$2`));
console.log(`\nNext: update CHANGELOG.md, commit, then \`git tag v${version} && git push origin v${version}\`.`);
