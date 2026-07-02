// Single source of truth for the compatibility table embedded in
// README.md and both usage guides — never hand-edited in three places.
// Run manually with `pnpm generate-table` (prints to stdout); the
// compat-check workflow calls this and writes the output between the
// `<!-- COMPAT_TABLE:START -->` / `<!-- COMPAT_TABLE:END -->` markers in
// each target file.
import { readFileSync } from 'node:fs';

const versions = JSON.parse(readFileSync(new URL('./VERSIONS.json', import.meta.url), 'utf8'));

const PACKAGE_NAMES = {
  'react-native': 'react-native',
  '@shopify/react-native-skia': '@shopify/react-native-skia',
  'react-native-reanimated': 'react-native-reanimated',
  'react-reconciler': 'react-reconciler',
};

const lines = [
  '| Package | Verified against | How |',
  '| --- | --- | --- |',
  ...Object.entries(versions.packages).map(([pkg, version]) => {
    const shimmed = pkg !== 'react-reconciler';
    const how = shimmed
      ? '[structural type check](../compat/) against real types + `e2e/` against real `@sc/ui`'
      : 'full test suite against the real package (no shim — used as-is)';
    return `| \`${PACKAGE_NAMES[pkg]}\` | \`${version}\` | ${how} |`;
  }),
  '',
  `_Last verified: ${versions.lastVerified}. Updated automatically by [.github/workflows/compat-check.yml](.github/workflows/compat-check.yml)._`,
];

console.log(lines.join('\n'));
