// A `react-native-reanimated`-compatible layer, aliased in at bundle time.
// Real reanimated runs worklets on a second "UI runtime" thread for perf,
// independent of React commits. We don't need that: we own the whole render
// loop single-threaded, so every "worklet" here just re-runs on our own
// per-frame tick (`__reanimatedTick`, called from rn-linux before each
// redraw) — cheap for the animation counts a desktop app has, and it
// sidesteps building a second Hermes runtime. `runOnUI`/`runOnJS` are
// therefore both just "call it now" — there's only one thread to be on.
import React from 'react';

import { View } from './react-native';

declare const __scSetStyle: (id: number, styleJson: string) => void;

export type SharedValue<T> = { value: T };

// ---- animation descriptors --------------------------------------------

type Step =
  | { kind: 'timing'; toValue: number; duration: number; easing: (t: number) => number }
  | { kind: 'spring'; toValue: number; stiffness: number; damping: number; mass: number }
  | { kind: 'decay'; velocity: number; deceleration: number };

const ANIM_TAG = Symbol('reanimated-animation');
type AnimDescriptor = {
  [ANIM_TAG]: true;
  steps: Step[];
  repeatCount: number; // 1 = no repeat, -1 = infinite
  reverse: boolean;
  callback?: (finished: boolean) => void;
};

function isAnimDescriptor(v: unknown): v is AnimDescriptor {
  return typeof v === 'object' && v !== null && ANIM_TAG in v;
}

function easeInOutQuad(t: number): number {
  return t < 0.5 ? 2 * t * t : 1 - Math.pow(-2 * t + 2, 2) / 2;
}

export function withTiming(toValue: number, config?: { duration?: number; easing?: (t: number) => number }, callback?: (finished: boolean) => void): number {
  // Typed as `number` (matching SharedValue<number>.value) so callers can
  // write `sv.value = withTiming(1)` without a cast — the real value is this
  // tagged descriptor, unwrapped by the shared-value setter below.
  const descriptor: AnimDescriptor = {
    [ANIM_TAG]: true,
    steps: [{ kind: 'timing', toValue, duration: config?.duration ?? 300, easing: config?.easing ?? easeInOutQuad }],
    repeatCount: 1,
    reverse: false,
    callback,
  };
  return descriptor as unknown as number;
}

export function withSpring(
  toValue: number,
  config?: { stiffness?: number; damping?: number; mass?: number },
  callback?: (finished: boolean) => void,
): number {
  const descriptor: AnimDescriptor = {
    [ANIM_TAG]: true,
    steps: [
      {
        kind: 'spring',
        toValue,
        stiffness: config?.stiffness ?? 100,
        damping: config?.damping ?? 10,
        mass: config?.mass ?? 1,
      },
    ],
    repeatCount: 1,
    reverse: false,
    callback,
  };
  return descriptor as unknown as number;
}

export function withDecay(config: { velocity: number; deceleration?: number }, callback?: (finished: boolean) => void): number {
  const descriptor: AnimDescriptor = {
    [ANIM_TAG]: true,
    steps: [{ kind: 'decay', velocity: config.velocity, deceleration: config.deceleration ?? 0.998 }],
    repeatCount: 1,
    reverse: false,
    callback,
  };
  return descriptor as unknown as number;
}

export function withSequence(...animations: number[]): number {
  const steps = animations.flatMap((a) => (isAnimDescriptor(a) ? a.steps : []));
  return { [ANIM_TAG]: true, steps, repeatCount: 1, reverse: false } as unknown as number;
}

export function withRepeat(animation: number, numberOfReps = -1, reverse = false, callback?: (finished: boolean) => void): number {
  const inner = isAnimDescriptor(animation) ? animation : null;
  return {
    [ANIM_TAG]: true,
    steps: inner?.steps ?? [],
    repeatCount: numberOfReps,
    reverse,
    callback,
  } as unknown as number;
}

// ---- shared value + tick-driven interpolation --------------------------

type RunningStep = {
  step: Step;
  from: number;
  start: number;
  // Spring: cached angular frequency so we don't recompute per-tick.
  omega0?: number;
  zeta?: number;
};

type Running = {
  descriptor: AnimDescriptor;
  stepIndex: number;
  current: RunningStep;
  iteration: number;
  baseFrom: number;
};

function nowMs(): number {
  return Date.now();
}

function startStep(step: Step, from: number): RunningStep {
  const running: RunningStep = { step, from, start: nowMs() };
  if (step.kind === 'spring') {
    const omega0 = Math.sqrt(step.stiffness / step.mass);
    running.omega0 = omega0;
    running.zeta = step.damping / (2 * Math.sqrt(step.stiffness * step.mass));
  }
  return running;
}

/** Returns `[value, finished]` for the running step at the current time. */
function evaluateStep(running: RunningStep): [number, boolean] {
  const elapsed = nowMs() - running.start;
  const { step, from } = running;
  if (step.kind === 'timing') {
    const t = Math.min(1, elapsed / step.duration);
    return [from + (step.toValue - from) * step.easing(t), elapsed >= step.duration];
  }
  if (step.kind === 'spring') {
    const omega0 = running.omega0!;
    const zeta = running.zeta!;
    const t = elapsed / 1000;
    let envelope: number;
    if (zeta < 1) {
      const wd = omega0 * Math.sqrt(1 - zeta * zeta);
      envelope = Math.exp(-zeta * omega0 * t) * (Math.cos(wd * t) + ((zeta * omega0) / wd) * Math.sin(wd * t));
    } else {
      // Critically/over-damped: no oscillation term, just exponential settle.
      envelope = Math.exp(-omega0 * t) * (1 + omega0 * t);
    }
    const value = step.toValue - (step.toValue - from) * envelope;
    // Settled once the envelope is negligible, capped so a pathological
    // config can't spin forever.
    const finished = Math.abs(envelope) < 0.001 || t > 8;
    return [finished ? step.toValue : value, finished];
  }
  // decay
  const t = elapsed / 1000;
  const decayPerSecond = step.deceleration;
  const value = from + (step.velocity * (1 - Math.pow(decayPerSecond, t))) / (1 - decayPerSecond) / 60;
  const currentVelocity = step.velocity * Math.pow(decayPerSecond, t);
  return [value, Math.abs(currentVelocity) < 0.1];
}

class SharedValueImpl<T> implements SharedValue<T> {
  private raw: T;
  private running: Running | null = null;

  constructor(initial: T) {
    this.raw = initial;
  }

  get value(): T {
    // `advance()` keeps `raw` current every tick while animating — see there.
    return this.raw;
  }

  set value(v: T) {
    activeAnimations.delete(this);
    this.running = null;
    if (isAnimDescriptor(v)) {
      if (v.steps.length === 0) {
        this.raw = v as unknown as T;
        return;
      }
      const from = this.raw as unknown as number;
      this.running = {
        descriptor: v,
        stepIndex: 0,
        current: startStep(v.steps[0], from),
        iteration: 0,
        baseFrom: from,
      };
      activeAnimations.add(this);
    } else {
      this.raw = v;
    }
  }

  /** Stops any in-flight animation, freezing at the current interpolated value. */
  cancel(): void {
    if (this.running) {
      const [value] = evaluateStep(this.running.current);
      this.raw = value as unknown as T;
    }
    this.running = null;
    activeAnimations.delete(this);
  }

  advance(): void {
    const r = this.running;
    if (!r) return;
    let [value, finished] = evaluateStep(r.current);
    this.raw = value as unknown as T;
    if (!finished) return;

    // Current step done — advance to the next step in the sequence, or loop.
    if (r.stepIndex + 1 < r.descriptor.steps.length) {
      r.stepIndex += 1;
      r.current = startStep(r.descriptor.steps[r.stepIndex], value);
      return;
    }

    r.iteration += 1;
    const repeatsLeft = r.descriptor.repeatCount < 0 || r.iteration < r.descriptor.repeatCount;
    if (repeatsLeft) {
      r.stepIndex = 0;
      const restartFrom = r.descriptor.reverse && r.iteration % 2 === 1 ? value : r.baseFrom;
      const firstStep = r.descriptor.steps[0];
      const target = r.descriptor.reverse && r.iteration % 2 === 1 ? r.baseFrom : (firstStep as { toValue?: number }).toValue ?? value;
      r.current = startStep(
        r.descriptor.reverse ? { ...(firstStep as Extract<Step, { toValue: number }>), toValue: target } : firstStep,
        restartFrom,
      );
      return;
    }

    this.running = null;
    activeAnimations.delete(this);
    r.descriptor.callback?.(true);
  }
}

const activeAnimations = new Set<SharedValueImpl<unknown>>();
const animatedStyleBindings = new Map<number, () => Record<string, unknown>>();
const reactions = new Set<{ prepare: () => unknown; react: (curr: unknown, prev: unknown) => void; prev: unknown }>();
const frameCallbacks = new Set<(info: { timestamp: number; timeSincePreviousFrame: number | null }) => void>();
let lastFrameTimestamp: number | null = null;

/** Called every frame from Rust (rn-linux) before layout+draw. */
(globalThis as Record<string, unknown>).__reanimatedTick = function reanimatedTick(): void {
  for (const sv of activeAnimations) sv.advance();
  for (const [instanceId, compute] of animatedStyleBindings) {
    __scSetStyle(instanceId, JSON.stringify(compute()));
  }
  // react-native-skia nodes with a SharedValue prop (e.g. `@sc/ui`'s idle
  // drift `<Group transform={useDerivedValue(...)}>`) — see hostConfig.ts.
  (globalThis as { __scRefreshAnimatedSkProps?: () => void }).__scRefreshAnimatedSkProps?.();
  // View/Canvas nodes whose `style` array embeds a `useAnimatedStyle()`
  // callback (`@sc/ui`'s Card/Button press-scale via `createAnimatedComponent`,
  // not `Animated.View`) — see hostConfig.ts.
  (globalThis as { __scRefreshAnimatedViewStyles?: () => void }).__scRefreshAnimatedViewStyles?.();
  for (const r of reactions) {
    const curr = r.prepare();
    if (curr !== r.prev) {
      r.react(curr, r.prev);
      r.prev = curr;
    }
  }
  const timestamp = nowMs();
  const delta = lastFrameTimestamp === null ? null : timestamp - lastFrameTimestamp;
  lastFrameTimestamp = timestamp;
  for (const cb of frameCallbacks) cb({ timestamp, timeSincePreviousFrame: delta });
};

export function useSharedValue<T>(initial: T): SharedValue<T> {
  const ref = React.useRef<SharedValueImpl<T> | null>(null);
  if (ref.current === null) ref.current = new SharedValueImpl(initial);
  return ref.current;
}

export function cancelAnimation(sharedValue: SharedValue<unknown>): void {
  (sharedValue as SharedValueImpl<unknown>).cancel?.();
}

/** Recomputed on every read — cheap arithmetic over shared values, no
 * dependency tracking needed (see module doc comment). `deps` is real
 * reanimated's API shape (`@sc/ui`'s `Atmosphere`/`Waveform` pass one) but
 * unused here for the same reason — always-fresh already covers it. */
export function useDerivedValue<T>(fn: () => T, _deps?: unknown[]): SharedValue<T> {
  const fnRef = React.useRef(fn);
  fnRef.current = fn;
  const holder = React.useRef<{ readonly value: T } | null>(null);
  if (holder.current === null) {
    holder.current = {
      get value(): T {
        return fnRef.current();
      },
    };
  }
  return holder.current;
}

export function useAnimatedReaction<T>(prepare: () => T, react: (current: T, previous: T | null) => void): void {
  const prepareRef = React.useRef(prepare);
  const reactRef = React.useRef(react);
  prepareRef.current = prepare;
  reactRef.current = react;

  React.useEffect(() => {
    const entry = {
      prepare: () => prepareRef.current(),
      react: (curr: unknown, prev: unknown) => reactRef.current(curr as T, prev as T | null),
      prev: null as unknown,
    };
    reactions.add(entry);
    return () => {
      reactions.delete(entry);
    };
  }, []);
}

export function useFrameCallback(callback: (info: { timestamp: number; timeSincePreviousFrame: number | null }) => void): { setActive: (active: boolean) => void } {
  const callbackRef = React.useRef(callback);
  callbackRef.current = callback;
  const stableRef = React.useRef<((info: { timestamp: number; timeSincePreviousFrame: number | null }) => void) | null>(null);
  if (!stableRef.current) stableRef.current = (info) => callbackRef.current(info);

  React.useEffect(() => {
    const fn = stableRef.current!;
    frameCallbacks.add(fn);
    return () => {
      frameCallbacks.delete(fn);
    };
  }, []);

  return React.useMemo(() => ({ setActive: () => {} }), []);
}

// Single-threaded: "run on UI" / "run on JS" both just mean "call it now".
export function runOnUI<A extends unknown[], R>(fn: (...args: A) => R): (...args: A) => void {
  return (...args: A) => {
    fn(...args);
  };
}
export const runOnJS = runOnUI;

export function useAnimatedStyle(fn: () => Record<string, unknown>, _deps?: unknown[]): () => Record<string, unknown> {
  const fnRef = React.useRef(fn);
  fnRef.current = fn;
  // Identity must stay stable across renders — `Animated.View` below keys its
  // tick registration on this function reference, not on props identity.
  const stable = React.useRef(() => fnRef.current());
  return stable.current;
}

// Same shape as useAnimatedStyle — real reanimated pairs this with
// `createAnimatedComponent`'s `animatedProps`, which we fold into style below.
export const useAnimatedProps = useAnimatedStyle;

// `@sc/ui`'s Card/Button wrap `Pressable` with this (not `Animated.View`) and
// embed the `useAnimatedStyle()` callback as one element of a `style` array
// (`style={[base, animatedStyle, style]}`) rather than passing it as the
// whole `style` value — `hostConfig.ts`'s `applyStyle`/`resolveStyle` already
// knows how to find and re-resolve a function anywhere inside a style array
// every tick, so this just needs to pass the array through unchanged instead
// of spreading it as if it were a plain object (which produced numeric-index
// garbage keys, silently dropping every real style property).
export function createAnimatedComponent<C extends React.ComponentType<{ style?: unknown }>>(
  Component: C,
): React.ComponentType<React.ComponentProps<C> & { animatedProps?: () => Record<string, unknown> }> {
  type P = React.ComponentProps<C>;
  // JSX only typechecks a generic component variable against its
  // constraint bound (`{ style?: unknown }`), not the wider inferred `C` —
  // recast to `C`'s actual prop type for the call below.
  const Comp = Component as unknown as React.ComponentType<P>;
  return function AnimatedComponent({ animatedProps, style, ...rest }: P & { animatedProps?: () => Record<string, unknown> }) {
    const mergedStyle = animatedProps ? [style, animatedProps] : style;
    return <Comp {...(rest as P)} style={mergedStyle} />;
  };
}

export function useAnimatedScrollHandler(_handlers: Record<string, (event: unknown) => void>): (event: unknown) => void {
  // No real scroll-position events yet (needs input plumbing) — kept for API
  // shape so importing screens don't crash.
  return () => {};
}

// ---- interpolation helpers ---------------------------------------------

export const Extrapolation = { EXTEND: 'extend', CLAMP: 'clamp', IDENTITY: 'identity' } as const;
export const Extrapolate = Extrapolation;

export function interpolate(
  value: number,
  inputRange: number[],
  outputRange: number[],
  extrapolate: 'extend' | 'clamp' | 'identity' = 'extend',
): number {
  let i = 0;
  while (i < inputRange.length - 2 && value > inputRange[i + 1]) i += 1;
  const inputMin = inputRange[i];
  const inputMax = inputRange[i + 1];
  const outputMin = outputRange[i];
  const outputMax = outputRange[i + 1];

  if (extrapolate === 'identity' && (value < inputRange[0] || value > inputRange[inputRange.length - 1])) {
    return value;
  }
  let t = (value - inputMin) / (inputMax - inputMin);
  if (extrapolate === 'clamp') t = Math.max(0, Math.min(1, t));
  return outputMin + t * (outputMax - outputMin);
}

export function interpolateColor(value: number, inputRange: number[], outputColorRange: [number, number, number, number][]): [number, number, number, number] {
  let i = 0;
  while (i < inputRange.length - 2 && value > inputRange[i + 1]) i += 1;
  const t = Math.max(0, Math.min(1, (value - inputRange[i]) / (inputRange[i + 1] - inputRange[i])));
  const a = outputColorRange[i];
  const b = outputColorRange[i + 1];
  return [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t, a[2] + (b[2] - a[2]) * t, a[3] + (b[3] - a[3]) * t];
}

// ---- Animated.View, the one host-config-aware piece --------------------

type AnimatedViewProps = Record<string, unknown> & {
  style?: unknown;
  children?: React.ReactNode;
};

const AnimatedView = React.forwardRef<number, AnimatedViewProps>((props, forwardedRef) => {
  const ownRef = React.useRef<number | null>(null);
  const { style, ...rest } = props;
  const isAnimated = typeof style === 'function';

  React.useEffect(() => {
    if (typeof forwardedRef === 'function') forwardedRef(ownRef.current);
    else if (forwardedRef) (forwardedRef as React.RefObject<number | null>).current = ownRef.current;
  });

  React.useEffect(() => {
    if (!isAnimated || ownRef.current === null) return;
    const id = ownRef.current;
    animatedStyleBindings.set(id, style as () => Record<string, unknown>);
    return () => {
      animatedStyleBindings.delete(id);
    };
  }, [isAnimated, style]);

  return <View ref={ownRef} style={isAnimated ? undefined : style} {...rest} />;
});

export const Animated = {
  View: AnimatedView,
  Text: AnimatedView,
  createAnimatedComponent,
};

export default Animated;
