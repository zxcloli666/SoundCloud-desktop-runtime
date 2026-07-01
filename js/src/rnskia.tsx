// Compatible subset of `@shopify/react-native-skia`'s JSX surface, aliased in
// at bundle time (see build.mjs). Real react-native-skia mounts a *second*,
// internal react-reconciler per <Canvas> that records an SkPicture and hands
// it to a native view; we own the whole pipeline, so these are just host
// types in our one outer tree (js-host/src/scene.rs draws them straight from
// props, no picture replay) — see Desktop-Runtime/CLAUDE.md for why.
//
// Covers broadly, not just what `@sc/ui` uses today. Exotic image-filter
// effects (BackdropBlur/BackdropFilter/Mask/ColorMatrix/Shader) degrade
// gracefully: they mount and lay out correctly but don't yet apply their
// visual effect (see js-host/src/scene.rs's NodeKind mapping) — real asset
// decoding (Image/useImage) and custom fonts (useFont) are follow-ups too.
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
export const Path = skNode('Path');
export const Text = skNode('Text');
export const Image = skNode('Image');
export const Paint = skNode('Paint');

// Degrade-gracefully stand-ins (see module doc comment): render/lay out
// correctly, effect itself not applied yet.
export const Shader = skNode('Shader');
export const ColorMatrix = skNode('ColorMatrix');
export const BackdropBlur = skNode('BackdropBlur');
export const BackdropFilter = skNode('BackdropFilter');
export const Mask = skNode('Mask');

// Pure geometry helpers — react-native-skia implements these as plain JS
// object constructors too, no native binding involved.
export type SkPoint = { x: number; y: number };
export type SkRect = { x: number; y: number; width: number; height: number };
export type SkRRect = { rect: SkRect; rx: number; ry: number };

// Type-only (`@sc/ui`'s `Atmosphere.tsx` imports it for a `useDerivedValue`
// return type) — matches real react-native-skia's shape closely enough for
// typechecking; erased at build time either way, no runtime behavior.
export type Transforms3d = (
  | { perspective: number }
  | { rotateX: number }
  | { rotateY: number }
  | { rotateZ: number }
  | { scale: number }
  | { scaleX: number }
  | { scaleY: number }
  | { translateX: number }
  | { translateY: number }
  | { skewX: number }
  | { skewY: number }
)[];

export const vec = (x: number, y: number): SkPoint => ({ x, y });
export const rect = (x: number, y: number, width: number, height: number): SkRect => ({
  x,
  y,
  width,
  height,
});
export const rrect = (r: SkRect, rx: number, ry: number): SkRRect => ({ rect: r, rx, ry });

// Imperative `Skia.*` facade — `@sc/ui` doesn't use it (declarative JSX only,
// see Desktop-Runtime/CLAUDE.md), but other code might reach for it. These
// are plain mutable JS objects, not real native Skia handles — good enough
// to build up prop values (color/paint/rect descriptors) for the declarative
// components above, not a general imperative canvas API.
class SkiaPaintStub {
  color: [number, number, number, number] = [0, 0, 0, 1];
  style: 'fill' | 'stroke' = 'fill';
  strokeWidth = 1;
  alpha = 1;
  setColor(c: [number, number, number, number]) {
    this.color = c;
  }
  setAlphaf(a: number) {
    this.alpha = a;
  }
  setStyle(s: 'fill' | 'stroke') {
    this.style = s;
  }
  setStrokeWidth(w: number) {
    this.strokeWidth = w;
  }
  copy() {
    return Object.assign(new SkiaPaintStub(), this);
  }
}

export const Skia = {
  Paint: () => new SkiaPaintStub(),
  Color: (value: string | [number, number, number, number]): [number, number, number, number] =>
    Array.isArray(value) ? value : [0, 0, 0, 1],
  RRectXY: (r: SkRect, rx: number, ry: number): SkRRect => rrect(r, rx, ry),
  XYWHRect: (x: number, y: number, width: number, height: number): SkRect => rect(x, y, width, height),
  Point: vec,
  Path: {
    Make: () => ({ svg: '', moveTo() {}, lineTo() {}, close() {}, toSVGString: () => '' }),
    MakeFromSVGString: (svg: string) => ({ svg, toSVGString: () => svg }),
  },
};

// Ticks once per real reanimated tick if a component reads `.value` inside a
// useDerivedValue/useAnimatedStyle (which re-run every tick regardless, see
// reanimated.tsx) — good enough for idle drift without a real vsync-driven
// clock source.
export function useClock(): { value: number } {
  const ref = React.useRef({ value: 0 });
  ref.current.value = Date.now();
  return ref.current;
}

// No asset-decoding pipeline yet — always "not loaded", matching the shape
// real react-native-skia returns while an image is in flight.
export function useImage(_source: unknown): null {
  return null;
}

// No custom font loading yet — components needing a font (e.g. a real
// <Text>) should fall back to the default size the Rust side already draws
// with when `font` is undefined.
export function useFont(_source: unknown, _size?: number): null {
  return null;
}

export function useVideo(_source: unknown): null {
  return null;
}
