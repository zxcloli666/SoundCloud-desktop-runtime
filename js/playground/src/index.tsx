// The engine's own zero-dependency demo — proves the whole pipeline
// (react-reconciler -> hostConfig -> Scene -> Yoga -> Skia) end to end
// without ever importing `@sc/ui`/`sc-rn`. This is what `rn-linux`'s
// default binary runs (`cargo run -p rn-linux`, after `pnpm build` here)
// with nothing but Desktop-Runtime on disk, and what the engine's own
// `js-host` tests (`tests/playground_bundle.rs`, `tests/reanimated.rs`,
// `tests/arbitrary_aspect_ratio.rs`) mount instead of a real `@sc/ui` bundle.
import React from 'react';
import Reconciler from 'react-reconciler';
import { ConcurrentRoot } from 'react-reconciler/constants';

import { hostConfig } from '../../src/hostConfig';
import { Image, Pressable, ScrollView, Text, View } from 'react-native';
import { Animated, useAnimatedStyle, useSharedValue, withTiming } from 'react-native-reanimated';
// esbuild's `dataurl` loader (build-support.mjs's `imageAssetLoaders`)
// turns this into a `data:image/png;base64,...` string at build time —
// exactly what a real `require('./photo.png')` resolves to on this
// runtime. `tests/playground_bundle.rs` decodes it back through the real
// pipeline (Image -> image_cache.rs -> Skia) to prove it end to end.
// @ts-expect-error -- no bundler type declarations for raw asset imports
import testAsset from './test-asset.png';

type Container = { rootId: number | null };
interface RealReconciler {
  createContainer(
    containerInfo: Container,
    tag: number,
    hydrationCallbacks: null,
    isStrictMode: boolean,
    concurrentUpdatesByDefaultOverride: null,
    identifierPrefix: string,
    onUncaughtError: (error: unknown) => void,
    onCaughtError: (error: unknown) => void,
    onRecoverableError: (error: unknown) => void,
    transitionCallbacks: null,
  ): unknown;
  updateContainer(
    element: React.ReactNode,
    container: unknown,
    parentComponent: null,
    callback: null,
  ): void;
}

const Renderer = Reconciler(
  hostConfig as unknown as Parameters<typeof Reconciler>[0],
) as unknown as RealReconciler;

// Root background — sampled by `tests/arbitrary_aspect_ratio.rs`. Distinct
// from the demo's own root color so the two can never be confused.
const ROOT_BACKGROUND: [number, number, number, number] = [0.02, 0.06, 0.1, 1.0];

function PressableTile({ label }: { label: string }) {
  return (
    <Pressable style={{ width: 80, height: 40, margin: 8, backgroundColor: [0.2, 0.5, 0.8, 1.0] }} onPress={() => {}}>
      <Text style={{ color: [1, 1, 1, 1], margin: 4 }}>{label}</Text>
    </Pressable>
  );
}

// A `require()`d local image, proven end to end: `testAsset` is the
// build-time `data:image/png;base64,...` string (see the import above),
// `tests/playground_bundle.rs` finds this node by its position/size and
// decodes its rendered pixels back through image_cache.rs -> Skia.
function RequiredAssetTile() {
  return <Image source={testAsset} style={{ width: 20, height: 20, margin: 8 }} />;
}

// Deliberately reproduces the exact column-outer/row-inner nesting shape
// that triggered a real bug (Desktop-Runtime/CLAUDE.md spike 8, item 8):
// Yoga's default `alignItems: stretch` clamped a horizontal ScrollView's
// content wrapper down to its container's own width, leaving nothing to
// scroll. `ScrollView` (react-native.tsx) already carries the fix
// (`alignSelf: 'flex-start'` on the content wrapper when `horizontal`) —
// this fixture exists so `tests/playground_bundle.rs` can guard it without
// needing the real `@sc/ui` `HorizontalScroll` block.
function OverflowCarousel() {
  return (
    <ScrollView horizontal style={{ width: 140, height: 60, margin: 8 }}>
      <View style={{ width: 100, height: 60, margin: 4, backgroundColor: [0.8, 0.3, 0.4, 1.0] }} />
      <View style={{ width: 100, height: 60, margin: 4, backgroundColor: [0.4, 0.8, 0.3, 1.0] }} />
      <View style={{ width: 100, height: 60, margin: 4, backgroundColor: [0.3, 0.4, 0.8, 1.0] }} />
    </ScrollView>
  );
}

// Mirrors the demo's `PulseBadge` (js/src/index.tsx) 1:1 — same target
// values, so `tests/reanimated.rs`'s assertions transfer over unchanged.
function PulseBox() {
  const width = useSharedValue(24);

  React.useEffect(() => {
    width.value = withTiming(220, { duration: 1200 });
  }, [width]);

  const style = useAnimatedStyle(() => ({
    width: width.value,
    height: 24,
    margin: 16,
    backgroundColor: [0.4, 0.9, 0.6, 1.0],
  }));

  return <Animated.View style={style} />;
}

// Three `key` swaps in a row, each forcing a real unmount + mount (not
// commitUpdate) — mirrors loading -> content -> content2. Reproduces the
// detachDeletedInstance bug (tests/screen_swap.rs): the first swap's
// deletion-effects crash doesn't break its OWN commit (the mutation phase
// already landed by the time the passive-effect flush throws) — it breaks
// the reconciler's scheduler state for the *next* one, so phase 2 never
// commits without the fix. A single swap alone doesn't catch this.
function ScreenSwap() {
  const [phase, setPhase] = React.useState(0);
  React.useEffect(() => {
    if (phase === 0) setPhase(1);
  }, [phase]);
  React.useEffect(() => {
    if (phase === 1) {
      const id = setTimeout(() => setPhase(2), 0);
      return () => clearTimeout(id);
    }
  }, [phase]);
  if (phase === 0) return <View key="phase0" style={{ width: 22, height: 22, margin: 8, backgroundColor: [0.8, 0.2, 0.2, 1.0] }} />;
  if (phase === 1) return <View key="phase1" style={{ width: 44, height: 44, margin: 8, backgroundColor: [0.9, 0.7, 0.1, 1.0] }} />;
  return <View key="phase2" style={{ width: 66, height: 66, margin: 8, backgroundColor: [0.2, 0.8, 0.3, 1.0] }} />;
}

function App() {
  return (
    <View style={{ backgroundColor: ROOT_BACKGROUND }}>
      <View style={{ flexDirection: 'row', flexWrap: 'wrap' }}>
        <PressableTile label="A" />
        <PressableTile label="B" />
        <RequiredAssetTile />
        <ScreenSwap />
      </View>
      <OverflowCarousel />
      {/* PulseBox stays last — tests/reanimated.rs finds it via
          `children_of(root).last()`, robust to however many siblings
          render before it, not to ones added after. */}
      <PulseBox />
    </View>
  );
}

const container: { rootId: number | null } = { rootId: null };

const throwFatal = (error: unknown) => {
  throw error;
};

const root = Renderer.createContainer(
  container,
  ConcurrentRoot,
  null,
  false,
  null,
  '',
  throwFatal,
  throwFatal,
  throwFatal,
  null,
);

Renderer.updateContainer(<App />, root, null, null);
