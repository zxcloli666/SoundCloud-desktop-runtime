// Fetches the latest published version of each tracked package from the
// npm registry and diffs against VERSIONS.json. Used both by
// .github/workflows/compat-check.yml and locally (`node
// check-latest-versions.mjs`) — plain `fetch`, no npm CLI dependency, so
// it behaves identically in both places.
import { readFileSync } from 'node:fs';

const versions = JSON.parse(readFileSync(new URL('./VERSIONS.json', import.meta.url), 'utf8'));

async function latestVersion(pkg) {
  const res = await fetch(`https://registry.npmjs.org/${encodeURIComponent(pkg).replace('%40', '@')}/latest`);
  if (!res.ok) throw new Error(`npm registry lookup for ${pkg} failed: HTTP ${res.status}`);
  const body = await res.json();
  return body.version;
}

const pkgNames = Object.keys(versions.packages);
const latest = Object.fromEntries(await Promise.all(pkgNames.map(async (pkg) => [pkg, await latestVersion(pkg)])));

const changed = pkgNames.filter((pkg) => latest[pkg] !== versions.packages[pkg]);

const result = { changed: changed.length > 0, changedPackages: changed, pinned: versions.packages, latest };
console.log(JSON.stringify(result, null, 2));

if (process.env.GITHUB_OUTPUT) {
  const { appendFileSync } = await import('node:fs');
  appendFileSync(process.env.GITHUB_OUTPUT, `changed=${result.changed}\n`);
  appendFileSync(process.env.GITHUB_OUTPUT, `latest_json=${JSON.stringify(latest)}\n`);
}
