// Thin wrappers so JSX (`<View>`, `<Text>`) resolves to our host-config's
// lowercase type strings — JSX only auto-strings intrinsic *lowercase* tags,
// capitalized tags need a real component reference. Stands in for the
// `react-native` core components until spike 7 aliases the real package.
// `forwardRef` matters here: reanimated's `Animated.View` needs a ref to the
// numeric instance id to register its per-frame style updates (see
// reanimated.tsx) — a plain function component can't receive one.
import React from 'react';

type Props = Record<string, unknown> & { children?: React.ReactNode };

export const View = React.forwardRef<number, Props>((props, ref) =>
  React.createElement('View', { ...props, ref }),
);

export function Text(props: Props) {
  return React.createElement('View', props, props.children);
}
