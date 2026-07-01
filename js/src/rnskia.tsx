// Compatible subset of `@shopify/react-native-skia`'s JSX surface — the exact
// components `@sc/ui` imports (see Desktop-Runtime/CLAUDE.md for the full
// scan). Real react-native-skia mounts a *second*, internal react-reconciler
// per <Canvas> that records an SkPicture and hands it to a native view; we
// own the whole pipeline, so these are just host types in our one outer tree
// (js-host/src/scene.rs draws them straight from props, no picture replay).
import React from 'react';

type Props = Record<string, unknown> & { children?: React.ReactNode };

function skNode(type: string) {
  return function SkNode(props: Props) {
    return React.createElement(type, props);
  };
}

export const Canvas = skNode('Canvas');
export const Group = skNode('Group');
export const Circle = skNode('Circle');
export const Rect = skNode('Rect');
export const RoundedRect = skNode('RoundedRect');
export const Blur = skNode('Blur');
export const RadialGradient = skNode('RadialGradient');
export const LinearGradient = skNode('LinearGradient');
export const Box = skNode('Box');
export const BoxShadow = skNode('BoxShadow');

// Pure geometry helpers — react-native-skia implements these as plain JS
// object constructors too, no native binding involved.
export type SkPoint = { x: number; y: number };
export type SkRect = { x: number; y: number; width: number; height: number };
export type SkRRect = { rect: SkRect; rx: number; ry: number };

export const vec = (x: number, y: number): SkPoint => ({ x, y });
export const rect = (x: number, y: number, width: number, height: number): SkRect => ({
  x,
  y,
  width,
  height,
});
export const rrect = (r: SkRect, rx: number, ry: number): SkRRect => ({ rect: r, rx, ry });

// Stand-in until spike 6 wires a real animation-frame-driven clock — @sc/ui's
// idle drift animation reads `.value` but doesn't require it to tick yet.
export function useClock(): { value: number } {
  return { value: 0 };
}
