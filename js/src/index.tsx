// Spike 5: the same glow-orbs + glass-panel look spike 2 hand-drew in Rust,
// this time composed declaratively from react-reconciler + our
// react-native-skia-compatible primitives (rnskia.tsx) — proves Circle/
// RadialGradient/Blur/Group/Box/BoxShadow/LinearGradient/RoundedRect, the
// exact subset `@sc/ui`'s Atmosphere/Waveform/GlassSurface use.
import React from 'react';
import Reconciler from 'react-reconciler';
import { LegacyRoot } from 'react-reconciler/constants';

import { hostConfig } from './hostConfig';
import { View } from './host-components';
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

function App() {
  return (
    <View style={{ backgroundColor: [0.04, 0.05, 0.08, 1.0] }}>
      <Scene />
    </View>
  );
}

const container: { rootId: number | null } = { rootId: null };

const throwFatal = (error: unknown) => {
  throw error;
};

const root = Renderer.createContainer(
  container,
  LegacyRoot,
  null,
  false,
  null,
  '',
  throwFatal,
  throwFatal,
  throwFatal,
  null,
);

// TODO(spike 4c, tracked in Desktop-Runtime task list): ConcurrentRoot —
// same mode real RN/Fabric uses — reaches `updateContainer` cleanly but never
// schedules a commit here; LegacyRoot + forced sync flush is the
// verified-working path for now.
Renderer.flushSyncFromReconciler(() => {
  Renderer.updateContainer(<App />, root, null, null);
});
