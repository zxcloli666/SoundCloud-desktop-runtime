// Mirrors the real react-native-reanimated surface @sc/ui actually
// imports (grepped from Button.tsx/Card.tsx/Atmosphere.tsx/Waveform.tsx):
// Animated.createAnimatedComponent, useSharedValue, useAnimatedStyle,
// useDerivedValue, withTiming, the SharedValue type. Compiled twice, same
// as the other snippets — see compat/README.md.
import { Pressable } from 'react-native';
import type { StyleProp, ViewStyle } from 'react-native';
import Animated, { useAnimatedStyle, useDerivedValue, useSharedValue, withTiming } from 'react-native-reanimated';
import type { SharedValue } from 'react-native-reanimated';

// Button.tsx / Card.tsx's exact shape.
const AnimatedPressable = Animated.createAnimatedComponent(Pressable);

function AnimatedButton({ style }: { style?: StyleProp<ViewStyle> }) {
  const pressed = useSharedValue(0);

  const animatedStyle = useAnimatedStyle(() => ({
    transform: [{ scale: 1 - pressed.value * 0.04 }],
    opacity: 1 - pressed.value * 0.1,
  }));

  return (
    <AnimatedPressable
      onPressIn={() => (pressed.value = withTiming(1, { duration: 120 }))}
      onPressOut={() => (pressed.value = withTiming(0, { duration: 180 }))}
      style={[animatedStyle, style]}
    />
  );
}

// Waveform.tsx's shape: a SharedValue<number> passed in as a prop and
// consumed by useDerivedValue.
function ProgressConsumer({ progress }: { progress: SharedValue<number> }) {
  const doubled = useDerivedValue(() => progress.value * 2, [progress]);
  return doubled;
}

export { AnimatedButton, ProgressConsumer };
