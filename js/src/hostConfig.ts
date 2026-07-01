// Mutation-mode react-reconciler host config, targeting the Rust `Scene`
// (js-host/src/scene.rs) through the __sc* globals js-host installs on the
// Hermes runtime. This is the "Fabric" side of our stack — but plain
// react-reconciler, not Meta's Fabric C++ (see Desktop-Runtime/CLAUDE.md).
declare const __scCreateView: () => number;
declare const __scCreateText: (text: string) => number;
declare const __scAppendChild: (parent: number, child: number) => void;
declare const __scRemoveChild: (parent: number, child: number) => void;
declare const __scSetStyle: (id: number, styleJson: string) => void;
declare const __scSetRoot: (id: number) => void;

import { DefaultEventPriority } from 'react-reconciler/constants';

// react-reconciler@0.32 also exports `NoEventPriority` (= 0) from this module,
// but @types/react-reconciler is pinned to 0.28 and doesn't know about it yet.
const NoEventPriority = 0;

export type ViewProps = {
  style?: Record<string, unknown>;
  children?: unknown;
};

type Instance = number;
type TextInstance = number;
type Container = { rootId: Instance | null };

function applyStyle(id: Instance, props: ViewProps): void {
  __scSetStyle(id, JSON.stringify(props.style ?? {}));
}

// React 18+ added priority tracking to the host config (not in react-reconciler's
// README, which predates it) — same pattern react-dom/react-native use: a module-level
// "current" priority set around event handling, defaulting when nothing is set.
let currentUpdatePriority = NoEventPriority;

export const hostConfig = {
  supportsMutation: true,
  supportsPersistence: false,
  supportsHydration: false,
  isPrimaryRenderer: true,
  supportsMicrotasks: true,
  scheduleMicrotask: queueMicrotask,
  noTimeout: -1,
  scheduleTimeout: setTimeout,
  cancelTimeout: clearTimeout,

  now: Date.now,
  getCurrentEventPriority: () => DefaultEventPriority,
  getCurrentUpdatePriority: () => currentUpdatePriority,
  setCurrentUpdatePriority: (priority: number) => {
    currentUpdatePriority = priority;
  },
  resolveUpdatePriority: () => currentUpdatePriority || DefaultEventPriority,

  createInstance(type: string, props: ViewProps): Instance {
    if (type !== 'View') throw new Error(`unknown host type: ${type}`);
    const id = __scCreateView();
    applyStyle(id, props);
    return id;
  },

  createTextInstance(text: string): TextInstance {
    return __scCreateText(text);
  },

  appendInitialChild(parent: Instance, child: Instance | TextInstance): void {
    __scAppendChild(parent, child);
  },

  finalizeInitialChildren(): boolean {
    return false;
  },

  shouldSetTextContent(): boolean {
    return false;
  },

  // react-reconciler's `requiredContext()` treats `null` as a bug signal (logs
  // "Expected host context to exist") despite its own README saying null is
  // fine — return a real (if unused) object instead.
  getRootHostContext(): Record<string, never> {
    return {};
  },

  getChildHostContext(parentContext: unknown): unknown {
    return parentContext;
  },

  getPublicInstance(instance: Instance): Instance {
    return instance;
  },

  prepareForCommit(): null {
    return null;
  },

  resetAfterCommit(container: Container): void {
    if (container.rootId !== null) __scSetRoot(container.rootId);
  },

  // React 19's "Suspensey commit" feature (CSS/image preloading) added these
  // to the required surface — not in react-reconciler's README, which
  // predates it, but `completeWork` calls `maySuspendCommit` unconditionally
  // for every host component. None of our host types ever suspend a commit.
  maySuspendCommit(): boolean {
    return false;
  },
  preloadInstance(): boolean {
    return true;
  },
  startSuspendingCommit(): void {},
  suspendInstance(): void {},
  waitForCommitToBeReady(): null {
    return null;
  },

  preparePortalMount(): void {},

  clearContainer(container: Container): void {
    container.rootId = null;
  },

  appendChild(parent: Instance, child: Instance | TextInstance): void {
    __scAppendChild(parent, child);
  },

  appendChildToContainer(container: Container, child: Instance): void {
    container.rootId = child;
    __scSetRoot(child);
  },

  insertBefore(parent: Instance, child: Instance, _beforeChild: Instance): void {
    // Our Scene only supports append, no explicit reordering yet — fine for
    // this spike's static tree; revisit once dynamic lists show up.
    __scAppendChild(parent, child);
  },

  insertInContainerBefore(container: Container, child: Instance): void {
    container.rootId = child;
    __scSetRoot(child);
  },

  removeChild(parent: Instance, child: Instance): void {
    __scRemoveChild(parent, child);
  },

  removeChildFromContainer(container: Container, child: Instance): void {
    if (container.rootId === child) container.rootId = null;
  },

  commitUpdate(instance: Instance, _type: string, _prevProps: ViewProps, nextProps: ViewProps): void {
    applyStyle(instance, nextProps);
  },

  commitTextUpdate(): void {
    throw new Error('text updates not supported yet');
  },

  hideInstance(): void {},
  hideTextInstance(): void {},
  unhideInstance(): void {},
  unhideTextInstance(): void {},
};
