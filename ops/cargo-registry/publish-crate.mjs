#!/usr/bin/env node
// Publishes one crate to the self-hosted sparse Cargo registry (see
// ../../docs/registry.md): packages it, appends an index entry (via
// `cargo metadata`, not hand-parsed TOML), and copies the .crate tarball
// out for a GitHub Release upload.
//
// Usage: node publish-crate.mjs <crate-dir> <registry-index-dir> <crate-output-dir>
import { execFileSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import { copyFileSync, existsSync, mkdirSync, readFileSync, appendFileSync } from 'node:fs';
import { join, resolve } from 'node:path';

const [, , crateDirArg, indexDirArg, outputDirArg] = process.argv;
if (!crateDirArg || !indexDirArg || !outputDirArg) {
  console.error('usage: node publish-crate.mjs <crate-dir> <registry-index-dir> <crate-output-dir>');
  process.exit(1);
}
const crateDir = resolve(crateDirArg);
const indexDir = resolve(indexDirArg);
const outputDir = resolve(outputDirArg);
mkdirSync(outputDir, { recursive: true });

function cargoMetadata(dir) {
  const raw = execFileSync('cargo', ['metadata', '--no-deps', '--format-version', '1'], { cwd: dir, encoding: 'utf8', maxBuffer: 64 * 1024 * 1024 });
  return JSON.parse(raw);
}

// Sparse-registry index path convention (Cargo's RFC 2789): by crate-name
// length, so lookups don't dump thousands of files into one directory.
function indexPathFor(name) {
  const n = name.length;
  if (n === 1) return `1/${name}`;
  if (n === 2) return `2/${name}`;
  if (n === 3) return `3/${name[0]}/${name}`;
  return `${name.slice(0, 2)}/${name.slice(2, 4)}/${name}`;
}

function sha256(path) {
  return createHash('sha256').update(readFileSync(path)).digest('hex');
}

const metadata = cargoMetadata(crateDir);
const pkg = metadata.packages.find((p) => p.manifest_path === join(crateDir, 'Cargo.toml'));
if (!pkg) throw new Error(`could not find package metadata for ${crateDir}`);

// Idempotent: a repeat workflow run must not double-append the version.
const indexFile = join(indexDir, indexPathFor(pkg.name));
if (existsSync(indexFile) && readFileSync(indexFile, 'utf8').split('\n').some((line) => line && JSON.parse(line).vers === pkg.version)) {
  console.log(`${pkg.name} v${pkg.version} is already indexed — skipping`);
  process.exit(0);
}

console.log(`packaging ${pkg.name} v${pkg.version}...`);
execFileSync('cargo', ['package', '--allow-dirty', '--no-verify'], { cwd: crateDir, stdio: 'inherit' });

// Never derive the target dir via relative path math — workspace nesting
// varies per crate; `target_directory` is always correct.
const tarball = join(metadata.target_directory, 'package', `${pkg.name}-${pkg.version}.crate`);
const outputTarball = join(outputDir, `${pkg.name}-${pkg.version}.crate`);
copyFileSync(tarball, outputTarball);

// Index format quirk: `registry: null` means "same registry as this
// entry", not "crates.io" — `cargo metadata`'s null (= default registry)
// must be translated explicitly, or plain deps like `cc`/`serde` would
// incorrectly resolve against our own registry too.
const CRATES_IO_INDEX = 'https://github.com/rust-lang/crates.io-index';

// publish.yml points CARGO_REGISTRIES_DESKTOP_RUNTIME_INDEX at an
// ephemeral local server for the duration of packaging (see serve-local.mjs
// and publish.yml's own comment) so a crate published earlier in the same
// run (e.g. skia-desktop, needed by rn-linux) resolves without waiting on
// real GitHub Pages propagation. `cargo metadata` reports THAT url for any
// `registry = "desktop-runtime"` dependency it resolves — baking it into
// the deployed index verbatim would leave every consumer's `cargo add`
// trying to resolve against a nonexistent localhost server (a real
// incident already hit this once for rusty_hermes — see docs/pitfalls/).
// Translate it back to the real, public registry URL before writing the
// index entry; a no-op when this env var isn't set (e.g. the fork's own
// registry, published without any local override).
const REAL_DESKTOP_RUNTIME_INDEX = 'sparse+https://zxcloli666.github.io/SoundCloud-desktop-runtime/registry/';
const LOCAL_OVERRIDE_INDEX = process.env.CARGO_REGISTRIES_DESKTOP_RUNTIME_INDEX;

const deps = pkg.dependencies
  .filter((d) => d.kind !== 'dev')
  .map((d) => ({
    name: d.rename ?? d.name,
    req: d.req,
    features: d.features,
    optional: d.optional,
    default_features: d.uses_default_features,
    target: d.target,
    kind: d.kind ?? 'normal',
    registry: d.registry === LOCAL_OVERRIDE_INDEX ? REAL_DESKTOP_RUNTIME_INDEX : (d.registry ?? CRATES_IO_INDEX),
    package: d.rename ? d.name : undefined,
  }));

const indexEntry = {
  name: pkg.name,
  vers: pkg.version,
  deps,
  cksum: sha256(outputTarball),
  features: pkg.features ?? {},
  yanked: false,
  links: pkg.links ?? null,
};

mkdirSync(join(indexFile, '..'), { recursive: true });
appendFileSync(indexFile, JSON.stringify(indexEntry) + '\n');

console.log(`indexed ${pkg.name} v${pkg.version} -> ${indexFile}`);
console.log(`tarball ready for release upload: ${outputTarball}`);
