// Mirrors the real react-native surface @sc/ui actually imports (grepped
// from Core/ui/src/{primitives,blocks}/*.tsx): View, Text, Image,
// Pressable, ScrollView, StyleSheet, useWindowDimensions, plus the prop/
// event types those files use. Compiled twice — once against the real
// `react-native` package (tsconfig.real.json), once with those imports
// aliased to js/src/react-native.tsx (tsconfig.shims.json) — both must
// pass clean. See compat/README.md.
import { Image, Pressable, ScrollView, StyleSheet, Text, View, useWindowDimensions } from 'react-native';
import type { GestureResponderEvent, LayoutChangeEvent, StyleProp, TextProps, TextStyle, ViewStyle } from 'react-native';

function ViewAndText() {
  const style: StyleProp<ViewStyle> = { flexDirection: 'row', gap: 12 };
  const textStyle: StyleProp<TextStyle> = { fontWeight: '600' };
  return (
    <View style={style}>
      <Text style={textStyle}>label</Text>
    </View>
  );
}

// Avatar.tsx / Card.tsx / TrackRow.tsx's shape: Image inside a Pressable/View.
function ImageInView({ uri }: { uri: string }) {
  return (
    <View>
      <Image source={{ uri }} resizeMode="cover" style={{ width: 40, height: 40, borderRadius: 20 }} />
    </View>
  );
}

// Button.tsx / Card.tsx / Waveform.tsx's shape: Pressable with an
// onPress-family handler receiving a real GestureResponderEvent.
function PressableExample({ onPress }: { onPress: (e: GestureResponderEvent) => void }) {
  return <Pressable onPress={onPress} style={{ padding: 8 }} />;
}

// HorizontalScroll.tsx's shape.
function HorizontalScrollExample() {
  return (
    <ScrollView horizontal showsHorizontalScrollIndicator={false} contentContainerStyle={{ gap: 12 }}>
      <View />
    </ScrollView>
  );
}

// GlassSurface.tsx's shape: StyleSheet.create + onLayout.
const styles = StyleSheet.create({
  surface: { overflow: 'hidden' },
});
function GlassSurfaceExample({ onLayout }: { onLayout: (e: LayoutChangeEvent) => void }) {
  return <View style={styles.surface} onLayout={onLayout} />;
}

// Atmosphere.tsx's shape.
function AtmosphereExample() {
  const { width, height } = useWindowDimensions();
  return <View style={{ width, height }} />;
}

// Text.tsx's exact shape: extends the real TextProps (minus 'role', which
// it redefines with its own design-token union) and spreads the rest onto
// the real <Text>.
interface WrappedTextProps extends Omit<TextProps, 'role'> {
  role?: 'hero' | 'display' | 'h1' | 'h2' | 'title' | 'body' | 'label' | 'caption';
}
function WrappedText({ role: _role, style, ...rest }: WrappedTextProps) {
  return <Text style={style} {...rest} />;
}

export { AtmosphereExample, GlassSurfaceExample, HorizontalScrollExample, ImageInView, PressableExample, ViewAndText, WrappedText };
