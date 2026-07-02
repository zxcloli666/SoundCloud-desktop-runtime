// Mirrors the real @shopify/react-native-skia surface @sc/ui actually
// imports (grepped from Atmosphere.tsx/GlassSurface.tsx/Waveform.tsx —
// the only 3 files in @sc/ui that touch Skia at all). Compiled twice, same
// as react-native-core.tsx — see compat/README.md.
import { Blur, Box, BoxShadow, Canvas, Circle, Group, LinearGradient, RadialGradient, Rect, RoundedRect, rect, rrect, useClock, vec } from '@shopify/react-native-skia';
import type { SkPoint, SkRRect, Transforms3d } from '@shopify/react-native-skia';
import { StyleSheet } from 'react-native';
import { useDerivedValue } from 'react-native-reanimated';

// Atmosphere.tsx's shape: Group(blendMode/opacity/transform) > Circle >
// RadialGradient(colors: string[]) + Blur, transform driven by useClock()
// through a Reanimated derived value.
function AtmosphereOrb({ center, r, color, amp }: { center: SkPoint; r: number; color: string; amp: number }) {
  const clock = useClock();
  const transform = useDerivedValue<Transforms3d>(() => {
    const t = (clock.value / 4000) * Math.PI * 2;
    return [{ translateX: Math.sin(t) * amp }, { translateY: Math.cos(t) * amp }];
  }, [clock, amp]);

  return (
    <Group blendMode="screen" opacity={0.8} transform={transform}>
      <Circle c={center} r={r}>
        <RadialGradient c={center} r={r} colors={[color, 'transparent']} />
        <Blur blur={30} />
      </Circle>
    </Group>
  );
}

function AtmosphereCanvas() {
  return (
    <Canvas style={StyleSheet.absoluteFill} pointerEvents="none">
      <AtmosphereOrb center={vec(100, 100)} r={80} color="#5a8cff" amp={12} />
    </Canvas>
  );
}

// GlassSurface.tsx's shape: Box(rrect(rect(...))) > LinearGradient(colors:
// string[], positions: number[]) + BoxShadow(inner), plus a stroked
// RoundedRect border and a sheen Rect > LinearGradient.
function GlassPaint({ w, h }: { w: number; h: number }) {
  return (
    <>
      <Box box={rrect(rect(0, 0, w, h), 16, 16)}>
        <LinearGradient
          start={vec(0, 0)}
          end={vec(w, h)}
          colors={['rgba(255,255,255,0.14)', 'rgba(255,255,255,0.04)']}
          positions={[0, 1]}
        />
        <BoxShadow dx={0} dy={8} blur={24} color="rgba(0,0,0,0.35)" inner={false} />
      </Box>
      <RoundedRect x={1} y={1} width={w - 2} height={h - 2} r={16} style="stroke" strokeWidth={1} color="rgba(255,255,255,0.28)" />
      <Rect x={16} y={0} width={w - 32} height={1.5}>
        <LinearGradient start={vec(16, 0)} end={vec(w - 16, 0)} colors={['transparent', '#ffffff', 'transparent']} />
      </Rect>
    </>
  );
}

// Waveform.tsx's shape: a SkRRect derived value clipping a Group of
// RoundedRects, recomputed off a Reanimated shared value.
function WaveformBars({ width, height, progress }: { width: number; height: number; progress: { value: number } }) {
  const clip = useDerivedValue<SkRRect>(() => rrect(rect(0, 0, width * progress.value, height), 0, 0), [width, height]);
  return (
    <Canvas style={{ flex: 1 }}>
      <Group>
        <RoundedRect x={0} y={0} width={3} height={height} r={1.5} color="rgba(255,255,255,0.2)" />
      </Group>
      <Group clip={clip}>
        <RoundedRect x={0} y={0} width={3} height={height} r={1.5} color="#5a8cff" />
      </Group>
    </Canvas>
  );
}

export { AtmosphereCanvas, AtmosphereOrb, GlassPaint, WaveformBars };
