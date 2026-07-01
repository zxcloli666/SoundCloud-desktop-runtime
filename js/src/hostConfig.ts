// Mutation-mode react-reconciler host config, targeting the Rust `Scene`
// (js-host/src/scene.rs) through the __sc* globals js-host installs on the
// Hermes runtime. This is the "Fabric" side of our stack — but plain
// react-reconciler, not Meta's Fabric C++ (see Desktop-Runtime/CLAUDE.md).
declare const __scCreateView: () => number;
declare const __scCreateText: (text: string) => number;
declare const __scSetText: (id: number, text: string) => void;
declare const __scCreateSkNode: (kind: string) => number;
declare const __scAppendChild: (parent: number, child: number) => void;
declare const __scRemoveChild: (parent: number, child: number) => void;
declare const __scSetStyle: (id: number, styleJson: string) => void;
declare const __scSetSkProps: (id: number, propsJson: string) => void;
declare const __scSetRoot: (id: number) => void;

// Everything except View/Text/Canvas is a Skia draw node (js-host/src/scene.rs)
// — no Yoga, raw props instead of flexbox style. Canvas itself IS a layout
// node (it's sized by flex like any View) but its *kind* on the Rust side is
// still "Canvas" so Scene knows its children are a Skia subtree, not more Views.
const SK_DRAW_TYPES = new Set([
  'Circle',
  'Rect',
  'RoundedRect',
  'Path',
  'Text',
  'Image',
  'Paint',
  'Group',
  'Blur',
  'RadialGradient',
  'LinearGradient',
  'Shader',
  'ColorMatrix',
  'BackdropBlur',
  'BackdropFilter',
  'Mask',
  'Box',
  'BoxShadow',
]);

import { DefaultEventPriority } from 'react-reconciler/constants';

import type { StyleProp } from './react-native';

// react-reconciler@0.32 also exports `NoEventPriority` (= 0) from this module,
// but @types/react-reconciler is pinned to 0.28 and doesn't know about it yet.
const NoEventPriority = 0;

// A style array element may itself be a function — reanimated's
// `useAnimatedStyle()`/`useAnimatedProps()` return a callback, and
// `Animated.createAnimatedComponent(X)` (used by `@sc/ui`'s Card/Button as
// `style={[base, animatedStyle, style]}`, not `Animated.View`) passes it
// through embedded in the array rather than as the whole `style` value.
type ViewStyleValue = StyleProp<Record<string, unknown>> | (() => Record<string, unknown>);

export type ViewProps = {
  style?: ViewStyleValue;
  children?: unknown;
};

type Instance = number;
type TextInstance = number;
type Container = { rootId: Instance | null };

function styleHasFunction(style: ViewStyleValue): boolean {
  if (typeof style === 'function') return true;
  if (Array.isArray(style)) return style.some((s) => styleHasFunction(s as ViewStyleValue));
  return false;
}

// Real RN components always pass `style` as an array (`style={[base, cond &&
// override]}`) and rely on the host platform to flatten it — Fabric/Paper do
// this internally, so it's invisible from JS. We're the "host platform"
// here: skip this and Rust's `serde_json::from_str::<StyleInput>` rejects
// the array outright, so no real `@sc/ui` component mounts at all. Also
// resolves any function element by calling it (see `ViewStyleValue` above).
function resolveStyle(style: ViewStyleValue): Record<string, unknown> {
  if (typeof style === 'function') return style();
  if (Array.isArray(style)) return Object.assign({}, ...style.map((s) => resolveStyle(s as ViewStyleValue)));
  return (style || {}) as Record<string, unknown>;
}

// id -> raw style (array/function form) for nodes whose style contains a
// reanimated callback and needs re-resolving every tick, not just at React
// commit time — same pattern as `skAnimatedNodes` below, for View/Canvas
// instead of Skia draw nodes.
const viewAnimatedNodes = new Map<Instance, ViewStyleValue>();

function applyStyle(id: Instance, props: ViewProps): void {
  const style = props.style;
  __scSetStyle(id, JSON.stringify(resolveStyle(style ?? {})));
  if (style !== undefined && styleHasFunction(style)) {
    viewAnimatedNodes.set(id, style);
  } else {
    viewAnimatedNodes.delete(id);
  }
}

// Called from reanimated.tsx's `__reanimatedTick`, alongside
// `__scRefreshAnimatedSkProps` below.
(globalThis as Record<string, unknown>).__scRefreshAnimatedViewStyles = function scRefreshAnimatedViewStyles(): void {
  for (const [id, style] of viewAnimatedNodes) {
    __scSetStyle(id, JSON.stringify(resolveStyle(style)));
  }
};

// react-native-skia lets *any* prop be a Reanimated `SharedValue` instead of a
// plain value (real react-native-skia reads `.value` at draw time via its own
// worklet runtime) — `@sc/ui`'s idle drift (`<Group transform={useDerivedValue(...)}>`)
// depends on this. Duck-typed rather than an `instanceof` check since
// `useDerivedValue`'s return value is a plain getter object, not a class.
function isSharedValueLike(v: unknown): v is { value: unknown } {
  return typeof v === 'object' && v !== null && !Array.isArray(v) && 'value' in v;
}

function resolveSkProps(rawProps: Record<string, unknown>): { resolved: Record<string, unknown>; hasSharedValue: boolean } {
  const resolved: Record<string, unknown> = {};
  let hasSharedValue = false;
  for (const [key, value] of Object.entries(rawProps)) {
    if (isSharedValueLike(value)) {
      hasSharedValue = true;
      resolved[key] = value.value;
    } else {
      resolved[key] = value;
    }
  }
  return { resolved, hasSharedValue };
}

// nodeId -> raw props (including any live SharedValues) for nodes that need
// re-resolving every reanimated tick, not just at React commit time.
const skAnimatedNodes = new Map<Instance, Record<string, unknown>>();

function applySkProps(id: Instance, props: Record<string, unknown>): void {
  const { children: _children, ...rawProps } = props;
  const { resolved, hasSharedValue } = resolveSkProps(rawProps);
  __scSetSkProps(id, JSON.stringify(resolved));
  if (hasSharedValue) {
    skAnimatedNodes.set(id, rawProps);
  } else {
    skAnimatedNodes.delete(id);
  }
}

// Called from reanimated.tsx's `__reanimatedTick` — same per-frame refresh
// pattern as `Animated.View`'s style bindings, just for Skia node props.
(globalThis as Record<string, unknown>).__scRefreshAnimatedSkProps = function scRefreshAnimatedSkProps(): void {
  for (const [id, rawProps] of skAnimatedNodes) {
    const { resolved } = resolveSkProps(rawProps);
    __scSetSkProps(id, JSON.stringify(resolved));
  }
};

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
    if (type === 'View') {
      const id = __scCreateView();
      applyStyle(id, props);
      return id;
    }
    if (type === 'Canvas') {
      const id = __scCreateSkNode('Canvas');
      applyStyle(id, props);
      return id;
    }
    if (SK_DRAW_TYPES.has(type)) {
      const id = __scCreateSkNode(type);
      applySkProps(id, props as Record<string, unknown>);
      return id;
    }
    throw new Error(`unknown host type: ${type}`);
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
    skAnimatedNodes.delete(child);
    viewAnimatedNodes.delete(child);
  },

  removeChildFromContainer(container: Container, child: Instance): void {
    if (container.rootId === child) container.rootId = null;
  },

  commitUpdate(instance: Instance, type: string, _prevProps: ViewProps, nextProps: ViewProps): void {
    if (type === 'View' || type === 'Canvas') {
      applyStyle(instance, nextProps);
    } else {
      applySkProps(instance, nextProps as Record<string, unknown>);
    }
  },

  commitTextUpdate(textInstance: TextInstance, _oldText: string, newText: string): void {
    __scSetText(textInstance, newText);
  },

  hideInstance(): void {},
  hideTextInstance(): void {},
  unhideInstance(): void {},
  unhideTextInstance(): void {},
};
