// Spike 6: a `react-native-reanimated`-compatible subset for the exact API
// `@sc/ui` uses (useSharedValue/useDerivedValue/useAnimatedStyle/withTiming/
// Animated.View — no withSpring/runOnUI/gestures, per the usage scan in
// Desktop-Runtime/CLAUDE.md). Real reanimated runs worklets on a second
// "UI runtime" thread for perf, independent of React commits. We don't need
// that: we own the whole render loop single-threaded, so every "worklet" here
// just re-runs on our own per-frame tick (`__reanimatedTick`, called from
// rn-linux before each redraw) — cheap for the handful of small animations a
// desktop app has, and it sidesteps building a second Hermes runtime.
import React from 'react';

import { View } from './host-components';

declare const __scSetStyle: (id: number, styleJson: string) => void;

export type SharedValue<T> = { value: T };

const TIMING_TAG = Symbol('reanimated-timing');
type TimingDescriptor = { [TIMING_TAG]: true; toValue: number; duration: number };

export function withTiming(toValue: number, config?: { duration?: number }): number {
  // Typed as `number` (matching SharedValue<number>.value) so callers can
  // write `sv.value = withTiming(1)` without a cast — the real value carried
  // is this tagged descriptor, unwrapped by the setter below.
  return { [TIMING_TAG]: true, toValue, duration: config?.duration ?? 300 } as unknown as number;
}

function isTimingDescriptor(v: unknown): v is TimingDescriptor {
  return typeof v === 'object' && v !== null && TIMING_TAG in v;
}

type ActiveAnimation = { from: number; to: number; start: number; duration: number };

class SharedValueImpl<T> implements SharedValue<T> {
  private raw: T;
  private anim: ActiveAnimation | null = null;

  constructor(initial: T) {
    this.raw = initial;
  }

  get value(): T {
    // `advance()` keeps `raw` current every tick while animating — see there.
    return this.raw;
  }

  set value(v: T) {
    if (isTimingDescriptor(v)) {
      this.anim = { from: this.raw as unknown as number, to: v.toValue, start: nowMs(), duration: v.duration };
      activeAnimations.add(this);
    } else {
      this.anim = null;
      this.raw = v;
      activeAnimations.delete(this);
    }
  }

  /** Current interpolated number while a timing animation is in flight. */
  currentNumber(): number {
    if (!this.anim) return this.raw as unknown as number;
    return this.anim.from + (this.anim.to - this.anim.from) * this.progress();
  }

  private progress(): number {
    const a = this.anim!;
    const t = Math.min(1, (nowMs() - a.start) / a.duration);
    // Default reanimated easing is ease-in-out; quad is close enough for our
    // narrow usage (idle drift, waveform reveal) without pulling in a curve library.
    return t < 0.5 ? 2 * t * t : 1 - Math.pow(-2 * t + 2, 2) / 2;
  }

  advance(): void {
    if (!this.anim) return;
    if (nowMs() - this.anim.start >= this.anim.duration) {
      this.raw = this.anim.to as unknown as T;
      this.anim = null;
      activeAnimations.delete(this);
    } else {
      this.raw = this.currentNumber() as unknown as T;
    }
  }
}

const activeAnimations = new Set<SharedValueImpl<unknown>>();
const animatedStyleBindings = new Map<number, () => Record<string, unknown>>();

function nowMs(): number {
  return Date.now();
}

/** Called every frame from Rust (rn-linux) before layout+draw. */
(globalThis as Record<string, unknown>).__reanimatedTick = function reanimatedTick(): void {
  for (const sv of activeAnimations) sv.advance();
  for (const [instanceId, compute] of animatedStyleBindings) {
    __scSetStyle(instanceId, JSON.stringify(compute()));
  }
};

export function useSharedValue<T>(initial: T): SharedValue<T> {
  const ref = React.useRef<SharedValueImpl<T> | null>(null);
  if (ref.current === null) ref.current = new SharedValueImpl(initial);
  return ref.current;
}

/** Recomputed on every read — cheap arithmetic over shared values, no
 * dependency tracking needed (see module doc comment). */
export function useDerivedValue<T>(fn: () => T): SharedValue<T> {
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

export function useAnimatedStyle(fn: () => Record<string, unknown>): () => Record<string, unknown> {
  const fnRef = React.useRef(fn);
  fnRef.current = fn;
  // Identity must stay stable across renders — `Animated.View` below keys its
  // tick registration on this function reference, not on props identity.
  const stable = React.useRef(() => fnRef.current());
  return stable.current;
}

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

export const Animated = { View: AnimatedView };
