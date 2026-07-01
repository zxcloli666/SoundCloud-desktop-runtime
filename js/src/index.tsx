// Spike 7a: the real `@sc/ui` package, unmodified, importing 'react-native' /
// '@shopify/react-native-skia' / 'react-native-reanimated' — resolved at
// bundle time (build.mjs `alias`) to our shims instead of the real native
// modules. `Atmosphere` below is @sc/ui's own component, not a local copy.
import { Atmosphere, Avatar, Button, Card, HorizontalScroll, SectionHeader, ThemeProvider, TrackRow } from '@sc/ui';
import React from 'react';
import Reconciler from 'react-reconciler';
import { ConcurrentRoot } from 'react-reconciler/constants';

import { hostConfig } from './hostConfig';
import { authStatus } from './live-data';
import { Text, View } from './react-native';
import { Animated, useAnimatedStyle, useSharedValue, withTiming } from './reanimated';
import { Blur, Box, BoxShadow, Canvas, Circle, Group, LinearGradient, RadialGradient, RoundedRect, rect, rrect, vec } from './rnskia';

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
  flushSyncFromReconciler<R>(fn: () => R): R;
}

const Renderer = Reconciler(
  hostConfig as unknown as Parameters<typeof Reconciler>[0],
) as unknown as RealReconciler;

function Scene() {
  const w = 700;
  const h = 320;
  return (
    <Canvas style={{ flexGrow: 1 }}>
      <Group blendMode="screen" opacity={0.8}>
        <Circle c={vec(260, 200)} r={140}>
          <RadialGradient c={vec(260, 200)} r={140} colors={[[0.35, 0.55, 1.0, 1.0], 'transparent']} />
          <Blur blur={30} />
        </Circle>
        <Circle c={vec(760, 420)} r={180}>
          <RadialGradient c={vec(760, 420)} r={180} colors={[[0.85, 0.4, 0.9, 1.0], 'transparent']} />
          <Blur blur={40} />
        </Circle>
      </Group>
      <Box box={rrect(rect(160, 160, w, h), 28, 28)}>
        <LinearGradient
          start={vec(160, 160)}
          end={vec(160 + w, 160 + h)}
          colors={[[1.0, 1.0, 1.0, 0.14], [1.0, 1.0, 1.0, 0.04]]}
        />
        <BoxShadow dx={0} dy={8} blur={24} color={[0.0, 0.0, 0.0, 0.35]} />
      </Box>
      <RoundedRect x={160} y={160} width={w} height={h} r={28} style="stroke" strokeWidth={1.5} color={[1.0, 1.0, 1.0, 0.28]} />
    </Canvas>
  );
}

// Proves spike 6: a shared value driven by `withTiming`, read back each frame
// through `useAnimatedStyle` and applied via the reanimated tick — no React
// re-render involved once mounted.
function PulseBadge() {
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

// Spike 7b: proves the whole sc-rn bridge end to end through the real
// bundle — not just the isolated Rust test in js-host/src/lib.rs. `rn-linux`
// calls `__scInitCore` before this ever mounts (see main.rs), so by the time
// `authStatus()`'s Promise resolves, the background tokio runtime has run a
// real `sc_rn::auth_status()` call and the result made it back through a
// live GPU frame loop, not a synchronous test harness.
function LiveDataProbe() {
  const [status, setStatus] = React.useState('sc-rn: loading…');

  React.useEffect(() => {
    let cancelled = false;
    authStatus()
      .then((s) => {
        if (!cancelled) setStatus(`sc-rn: hasSession=${s.hasSession} authenticated=${s.authenticated}`);
      })
      .catch((e: Error) => {
        if (!cancelled) setStatus(`sc-rn error: ${e.message}`);
      });
    return () => {
      cancelled = true;
    };
  }, []);

  return <Text style={{ margin: 16, color: [1.0, 1.0, 1.0, 0.85] }}>{status}</Text>;
}

// Exercises `@sc/ui`'s actual blocks — not the raw shim primitives `Scene`/
// `LiveDataProbe` use above. These are the components that hit the
// array-form `style` bugs found auditing shim coverage: `Card`/`Button` wrap
// `Pressable` via `Animated.createAnimatedComponent` and embed a
// `useAnimatedStyle()` callback inside a `style` array, `Avatar`/`TrackRow`
// pass `style={[...]}` directly. If this renders with real backgrounds/
// border-radii/gaps (not blank boxes), the style-array + animated-style
// fixes actually work end to end, not just in isolation.
function CoreUiProbe() {
  return (
    <View style={{ gap: 12, margin: 16 }}>
      <SectionHeader kicker="Проверка" title="Core/ui блоки" />
      <View style={{ flexDirection: 'row', gap: 12, alignItems: 'center' }}>
        <Avatar url={null} size={56} ring />
        <Button label="Слушать" onPress={() => {}} />
      </View>
      <Card title="Тестовый трек" subtitle="Тестовый артист" width={140} onPress={() => {}} />
      <TrackRow index={0} title="Первый трек" artist="Артист" durationMs={183000} active onPress={() => {}} />
      {/* Exercises ScrollView's contentContainerStyle (gap/edge padding) —
          previously silently dropped (only `style` was read). */}
      <HorizontalScroll gap={12} edge={16}>
        <Card title="Карточка A" width={100} />
        <Card title="Карточка B" width={100} />
        <Card title="Карточка C" width={100} />
      </HorizontalScroll>
    </View>
  );
}

function App() {
  return (
    <ThemeProvider accent="#5a8cff" perfMode="beauty">
      <View style={{ backgroundColor: [0.04, 0.05, 0.08, 1.0] }}>
        {/* Real @sc/ui component, unmodified — proves the bundler-alias
            swap (build.mjs) works end to end, not just our own test scene. */}
        <Atmosphere />
        <Scene />
        <LiveDataProbe />
        <CoreUiProbe />
        {/* PulseBadge stays last — reanimated_test (js-host/src/lib.rs) finds
            it via `children_of(root).last()`, robust to however many
            siblings render before it, not to ones added after. */}
        <PulseBadge />
      </View>
    </ThemeProvider>
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

// ConcurrentRoot — the same mode real RN/Fabric uses. Previously this reached
// `updateContainer` but never scheduled a commit; root-caused (see
// js-host/src/host.rs's PRELUDE_JS) to the `setTimeout`/`setImmediate` shims
// running their callback inline instead of deferring — the `scheduler`
// package schedules Concurrent-mode work through exactly that primitive, so
// an inline-synchronous shim meant the scheduled callback either never ran
// on its own or re-entered mid-commit. Now that timers genuinely defer to
// `__scDrainTimers()` (rn-linux's render loop, once per frame), plain
// `updateContainer` — no forced sync flush — schedules and commits normally.
Renderer.updateContainer(<App />, root, null, null);
