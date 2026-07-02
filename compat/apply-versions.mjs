// Writes new versions into VERSIONS.json + compat/package.json's
// devDependencies, then regenerates every doc that embeds the
// compatibility table. Only ever run AFTER `pnpm check` has proven the
// new versions still pass — see .github/workflows/compat-check.yml. Takes
// the new versions as a JSON object on argv[2] (matches
// check-latest-versions.mjs's `latest` field).
import { execFileSync } from 'node:child_process';
import { readFileSync, writeFileSync } from 'node:fs';

const latest = JSON.parse(process.argv[2] ?? '{}');
if (Object.keys(latest).length === 0) {
  throw new Error('usage: node apply-versions.mjs \'{"react-native":"0.87.0",...}\'');
}

const versionsPath = new URL('./VERSIONS.json', import.meta.url);
const versions = JSON.parse(readFileSync(versionsPath, 'utf8'));
versions.packages = { ...versions.packages, ...latest };
versions.lastVerified = new Date().toISOString().slice(0, 10);
writeFileSync(versionsPath, JSON.stringify(versions, null, 2) + '\n');

const pkgPath = new URL('./package.json', import.meta.url);
const pkg = JSON.parse(readFileSync(pkgPath, 'utf8'));
for (const [name, version] of Object.entries(latest)) {
  if (pkg.devDependencies[name]) pkg.devDependencies[name] = version;
}
writeFileSync(pkgPath, JSON.stringify(pkg, null, 2) + '\n');

// Regenerate the embedded compatibility table in every doc that carries
// the COMPAT_TABLE markers.
const table = execFileSync('node', ['generate-table.mjs'], { cwd: new URL('.', import.meta.url), encoding: 'utf8' });
const targets = ['../README.md', '../docs/usage.md', '../docs/usage.ru.md'];
const START = '<!-- COMPAT_TABLE:START -->';
const END = '<!-- COMPAT_TABLE:END -->';
for (const rel of targets) {
  const target = new URL(rel, import.meta.url);
  let content;
  try {
    content = readFileSync(target, 'utf8');
  } catch {
    continue; // doc doesn't exist yet (or doesn't carry the table) — skip, not an error
  }
  const start = content.indexOf(START);
  const end = content.indexOf(END);
  if (start === -1 || end === -1) continue;
  const before = content.slice(0, start + START.length);
  const after = content.slice(end);
  writeFileSync(target, `${before}\n${table.trim()}\n${after}`);
}

console.log(`VERSIONS.json + compat/package.json updated: ${JSON.stringify(latest)}`);
