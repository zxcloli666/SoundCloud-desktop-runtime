// Thin wrappers so JSX (`<View>`, `<Text>`) resolves to our host-config's
// lowercase type strings — JSX only auto-strings intrinsic *lowercase* tags,
// capitalized tags need a real component reference. Stands in for the
// `react-native` core components until spike 7 aliases the real package.
import React from 'react';

type Props = Record<string, unknown> & { children?: React.ReactNode };

export function View(props: Props) {
  return React.createElement('View', props);
}

export function Text(props: Props) {
  return React.createElement('View', props, props.children);
}
