// Single source of truth for the compatibility table embedded in
// README.md — never hand-edited. Run manually with `pnpm generate-table`
// (prints to stdout); the compat-check workflow calls this and writes the
// output between the `<!-- COMPAT_TABLE:START -->` / `<!-- COMPAT_TABLE:
// END -->` markers.
import { readFileSync } from 'node:fs';

const versions = JSON.parse(readFileSync(new URL('./VERSIONS.json', import.meta.url), 'utf8'));

const PACKAGE_NAMES = {
  'react-native': 'react-native',
  '@shopify/react-native-skia': '@shopify/react-native-skia',
  'react-native-reanimated': 'react-native-reanimated',
  'react-reconciler': 'react-reconciler',
};

const lines = [
  '| Package | Status | Current | Verified since | Track record |',
  '| --- | :---: | --- | --- | --- |',
  ...Object.entries(versions.packages).map(([pkg, { current, history }]) => {
    const since = history[0].verifiedAt;
    const trail = history.map((h) => `\`${h.version}\``).join(' → ');
    return `| \`${PACKAGE_NAMES[pkg]}\` | ✅ | \`${current}\` | ${since} | ${trail} |`;
  }),
];

console.log(lines.join('\n'));
