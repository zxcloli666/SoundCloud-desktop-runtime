// Spike 4b: the same visual proof as rn-linux's hand-written SCENE_JS, this
// time built by real React + react-reconciler instead of direct host-function
// calls — proves the reconciler wiring, not just the mounting layer under it.
import React from 'react';
import Reconciler from 'react-reconciler';
import { LegacyRoot } from 'react-reconciler/constants';

import { hostConfig } from './hostConfig';

// react-reconciler@0.32 (React 19) splits onCaughtError from onUncaughtError
// in createContainer; @types/react-reconciler is pinned to 0.28's 8-arg shape
// and hasn't caught up, so we widen just this one method to match reality.
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

function App() {
  return React.createElement(
    'View',
    {
      style: {
        flexDirection: 'column',
        padding: 24,
        backgroundColor: [0.04, 0.05, 0.08, 1.0],
      },
    },
    React.createElement(
      'View',
      {
        style: {
          flexDirection: 'row',
          padding: 16,
          backgroundColor: [1.0, 1.0, 1.0, 0.1],
        },
      },
      React.createElement('View', {
        style: { width: 64, height: 64, backgroundColor: [0.35, 0.55, 1.0, 0.9] },
      }),
      React.createElement(
        'View',
        { style: { margin: 16 } },
        'react-reconciler is driving this, not hand-written JS',
      ),
    ),
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

// TODO(spike 4c): ConcurrentRoot — same mode real RN/Fabric uses — reached
// `updateContainer` cleanly but never actually scheduled a commit here (no
// microtask enqueued, no error); LegacyRoot + forced sync flush is the
// verified-working path for now. Investigate before spike 6 (reanimated
// worklets likely assume concurrent scheduling semantics).
Renderer.flushSyncFromReconciler(() => {
  Renderer.updateContainer(React.createElement(App), root, null, null);
});
