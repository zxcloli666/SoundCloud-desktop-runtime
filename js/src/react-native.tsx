// Compatible subset of the `react-native` package, aliased in at bundle time
// (see build.mjs) so `@sc/ui` and screens import the real package name and
// never know they're not running on Android/iOS. Covers the common surface
// broadly, not just what `@sc/ui` happens to use today — cheaper to stub a
// component now than to come back mid-feature-work later.
//
// Real interactivity (press/scroll/text-input) needs actual pointer/keyboard
// events routed from winit into the reconciler, which doesn't exist yet —
// those components render correctly but don't yet respond to input.
import React from 'react';

// Type-only exports `@sc/ui` imports from 'react-native' — esbuild erases
// `import type` before module resolution, so a missing export here is
// invisible until something actually typechecks `js/` against these shims
// (no tsconfig/typecheck step exists yet). Kept loose (not the real
// packages' precise shapes) since nothing here affects runtime behavior.
export type StyleProp<T> = T | StyleProp<T>[] | null | undefined | false;
export type ViewStyle = Record<string, unknown>;
export type TextStyle = Record<string, unknown>;
export interface TextProps {
  style?: StyleProp<TextStyle>;
  numberOfLines?: number;
  children?: React.ReactNode;
}
export interface LayoutChangeEvent {
  nativeEvent: { layout: { x: number; y: number; width: number; height: number } };
}
export interface GestureResponderEvent {
  nativeEvent: { locationX: number; locationY: number; pageX: number; pageY: number };
}

// Deliberately no `Record<string, unknown>` wildcard here (tried it, kept
// it consistent — with one, TypeScript can no longer contextually infer
// callback parameter types like `onLayout`'s `event` at JSX call sites,
// silently falling back to implicit `any` instead of erroring, which is
// worse than just not modeling a prop `@sc/ui` doesn't currently use).
// Extend this list as real usage needs more (accessibility props, testID,
// hitSlop, ...) rather than reaching for a blanket index signature again.
type Props = {
  children?: React.ReactNode;
  style?: unknown;
  onLayout?: (event: LayoutChangeEvent) => void;
  testID?: string;
};

export const View = React.forwardRef<number, Props>((props, ref) =>
  React.createElement('View', { ...props, ref }),
);

export const Text = React.forwardRef<number, Props>((props, ref) =>
  React.createElement('View', { ...props, ref }, props.children as React.ReactNode),
);

// `require('./photo.png')`/`import photo from './photo.png'` resolve to a
// `data:` URI string directly — js/build-support.mjs's `imageAssetLoaders()`
// maps image extensions to esbuild's built-in `dataurl` loader, embedding
// the file into the bundle at build time (no separate asset server needed
// on this desktop runtime, unlike Metro's numeric-asset-ID registry on
// mobile). A bare string source is therefore just as real as `{ uri }`.
type ImageSource = { uri?: string | null } | string | null | undefined;
type ImageResizeMode = 'cover' | 'contain' | 'stretch' | 'center' | 'repeat';

// A real fetch+decode (image_cache.rs) either way — `source`/`resizeMode`
// fold into `style` as `imageUri`/`imageResizeMode` (same synthetic-style-
// key trick `ScrollView` uses for `scrollable`), since that's the channel
// `__scSetStyle` already has to the Rust Scene. `image_cache.rs`'s `fetch`
// decodes a `data:` URI locally instead of issuing a network request.
export const Image = React.forwardRef<number, Props & { source?: ImageSource; resizeMode?: ImageResizeMode }>((props, ref) => {
  const { source, resizeMode, style, ...rest } = props;
  const uri = typeof source === 'string' ? source : (source?.uri ?? undefined);
  return React.createElement('View', {
    ...rest,
    style: [style, uri ? { imageUri: uri, imageResizeMode: resizeMode ?? 'cover' } : null],
    ref,
  });
});

type PressableProps = Props & {
  onPress?: (event: GestureResponderEvent) => void;
  onPressIn?: (event: GestureResponderEvent) => void;
  onPressOut?: (event: GestureResponderEvent) => void;
  onLongPress?: (event: GestureResponderEvent) => void;
};

// `onPress`/`onPressIn`/`onPressOut`/`onLongPress` pass straight through to
// the host 'View' type unchanged — hostConfig.ts reads them off `props`
// directly (real pointer hit-testing from rn-linux's winit event loop,
// dispatched through __scWatchPress/__scDispatchPress) rather than Pressable
// doing anything itself. `TouchableOpacity`/`TouchableHighlight`/
// `TouchableWithoutFeedback` inherit this the same way real RN's do, being
// Pressable-based wrappers themselves.
export const Pressable = React.forwardRef<number, PressableProps>((props, ref) =>
  React.createElement('View', { ...props, ref }),
);

export const TouchableOpacity = Pressable;
export const TouchableHighlight = Pressable;
export const TouchableWithoutFeedback = Pressable;

type ScrollViewProps = Props & {
  horizontal?: boolean;
  contentContainerStyle?: unknown;
  showsHorizontalScrollIndicator?: boolean;
  showsVerticalScrollIndicator?: boolean;
  decelerationRate?: 'normal' | 'fast' | number;
  snapToInterval?: number;
  snapToAlignment?: 'start' | 'center' | 'end';
  onScroll?: (event: { nativeEvent: { contentOffset: { x: number; y: number } } }) => void;
};

// Applies `overflow: hidden` like the real component's clipsToBounds
// default, and lays children out `flexDirection: row` when `horizontal` —
// `contentContainerStyle` (real RN applies it to an inner wrapper View
// around the content, separate from `style` on the outer scroll clip
// container — `HorizontalScroll`'s gap/edge-padding lives there) is honored
// the same way. `scrollable`/`scrollHorizontal` (StyleInput, scene.rs) mark
// the outer node as a real mouse-wheel scroll target — Scene owns the
// actual scroll position (rn-linux's winit loop hit-tests + calls
// Scene::scroll_by on MouseWheel), not React state, so it doesn't round-trip
// through a re-render for every wheel tick.
export const ScrollView = React.forwardRef<number, ScrollViewProps>((props, ref) => {
  const { style, contentContainerStyle, horizontal, children, ...rest } = props;
  return React.createElement(
    'View',
    { style: [style, { overflow: 'hidden', scrollable: true, scrollHorizontal: !!horizontal }], ref },
    React.createElement(
      'View',
      {
        // `alignSelf: 'flex-start'` only for `horizontal` — the content
        // wrapper is a column-direction child of the container above, so
        // its width is that column's cross axis, which Yoga's default
        // `alignItems: stretch` would otherwise clamp to the container's own
        // width — exactly the one dimension a horizontal scroll's content
        // needs to size naturally to its row-direction children's combined
        // width instead (there'd be nothing to scroll otherwise). A
        // vertical scroll's content wrapper *should* stretch to the
        // container's width — only its height (the main axis, unaffected
        // by alignItems either way) needs to grow past the container.
        style: [contentContainerStyle, horizontal ? { flexDirection: 'row', alignSelf: 'flex-start' } : null],
        ...rest,
      },
      children as React.ReactNode,
    ),
  );
});

type ListProps<T> = Props & {
  data?: T[];
  renderItem?: (info: { item: T; index: number }) => React.ReactNode;
  keyExtractor?: (item: T, index: number) => string;
  ListHeaderComponent?: React.ReactNode;
  ListFooterComponent?: React.ReactNode;
  ListEmptyComponent?: React.ReactNode;
};

// No virtualization (desktop screens are small enough that it's not worth
// the complexity yet) — just maps `data` through `renderItem` inside a View.
export function FlatList<T>(props: ListProps<T>) {
  const { data, renderItem, keyExtractor, ListHeaderComponent, ListFooterComponent, ListEmptyComponent, style } = props;
  const items = data ?? [];
  return (
    <View style={style}>
      {ListHeaderComponent}
      {items.length === 0
        ? ListEmptyComponent
        : items.map((item, index) => (
            <React.Fragment key={keyExtractor ? keyExtractor(item, index) : index}>
              {renderItem?.({ item, index })}
            </React.Fragment>
          ))}
      {ListFooterComponent}
    </View>
  );
}

type SectionListProps<T> = Props & {
  sections?: Array<{ title?: string; data: T[] }>;
  renderItem?: (info: { item: T; index: number }) => React.ReactNode;
  renderSectionHeader?: (info: { section: { title?: string; data: T[] } }) => React.ReactNode;
  keyExtractor?: (item: T, index: number) => string;
};

export function SectionList<T>(props: SectionListProps<T>) {
  const { sections, renderItem, renderSectionHeader, keyExtractor, style } = props;
  return (
    <View style={style}>
      {(sections ?? []).map((section, sectionIndex) => (
        <React.Fragment key={section.title ?? sectionIndex}>
          {renderSectionHeader?.({ section })}
          {section.data.map((item, index) => (
            <React.Fragment key={keyExtractor ? keyExtractor(item, index) : `${sectionIndex}-${index}`}>
              {renderItem?.({ item, index })}
            </React.Fragment>
          ))}
        </React.Fragment>
      ))}
    </View>
  );
}

export function SafeAreaView(props: Props) {
  // Single desktop window, no notches/insets to account for.
  return <View {...props} />;
}

export function KeyboardAvoidingView(props: Props) {
  return <View {...props} />;
}

// Renders as a static box — no spinner rotation. Not used by @sc/ui today.
export function ActivityIndicator({ size = 20, color = [1, 1, 1, 1] }: { size?: number | 'small' | 'large'; color?: unknown }) {
  const px = size === 'small' ? 20 : size === 'large' ? 36 : size;
  return <View style={{ width: px, height: px, borderRadius: px / 2, backgroundColor: color }} />;
}

// Renders as a plain box — no interactive thumb/track animation without
// input plumbing.
export function Switch({ value, trackColor }: { value?: boolean; trackColor?: unknown }) {
  return (
    <View
      style={{
        width: 40,
        height: 24,
        borderRadius: 12,
        backgroundColor: value ? (trackColor ?? [0.4, 0.9, 0.6, 1.0]) : [0.3, 0.3, 0.3, 1.0],
      }}
    />
  );
}

// Renders a box the same shape as a text field; not focusable/editable yet
// (needs keyboard-event plumbing from winit).
export function TextInput(props: Props & { value?: string; placeholder?: string }) {
  const { value, placeholder, style } = props;
  return (
    <View style={{ height: 40, padding: 8, borderRadius: 6, borderWidth: 1, borderColor: [0.4, 0.4, 0.4, 1.0], ...(style as object) }}>
      <Text>{value || placeholder || ''}</Text>
    </View>
  );
}

// No overlay/z-order compositor yet — renders in place rather than above
// everything else. Fine for now since nothing uses Modal yet; revisit
// alongside a real z-index/portal system.
export function Modal({ visible = true, children }: Props & { visible?: boolean }) {
  return visible ? <View style={{ position: 'absolute', left: 0, right: 0, top: 0, bottom: 0 }}>{children}</View> : null;
}

export function StatusBar(_props: Record<string, unknown>) {
  return null;
}
StatusBar.setBarStyle = () => {};
StatusBar.setBackgroundColor = () => {};
StatusBar.setHidden = () => {};

export const Alert = {
  alert: (title: string, message?: string) => {
    console.warn(`[Alert] ${title}${message ? `: ${message}` : ''}`);
  },
};

export const Keyboard = {
  dismiss: () => {},
  addListener: () => ({ remove: () => {} }),
};

export const AppState = {
  currentState: 'active' as const,
  addEventListener: () => ({ remove: () => {} }),
};

export const BackHandler = {
  addEventListener: () => ({ remove: () => {} }),
  removeEventListener: () => {},
  exitApp: () => {},
};

export const Linking = {
  openURL: async (_url: string) => {},
  canOpenURL: async (_url: string) => true,
  addEventListener: () => ({ remove: () => {} }),
};

export const StyleSheet = {
  create<T extends Record<string, unknown>>(styles: T): T {
    return styles;
  },
  flatten<T extends Record<string, unknown>>(style: StyleProp<T>): T {
    if (Array.isArray(style)) {
      return Object.assign({}, ...style.map((s) => StyleSheet.flatten(s))) as T;
    }
    return (style || ({} as T));
  },
  compose<T>(a: T, b: T): T[] {
    return [a, b];
  },
  hairlineWidth: 1,
  absoluteFillObject: { position: 'absolute', left: 0, right: 0, top: 0, bottom: 0 },
  get absoluteFill() {
    return this.absoluteFillObject;
  },
};

export const Platform = {
  OS: 'linux' as const,
  select<T>(specifics: { linux?: T; default?: T; native?: T }): T | undefined {
    return specifics.linux ?? specifics.default;
  },
  Version: 1,
};

export const PixelRatio = {
  get: () => 1,
  getFontScale: () => 1,
  getPixelSizeForLayoutSize: (n: number) => Math.round(n),
  roundToNearestPixel: (n: number) => Math.round(n),
};

type Size = { width: number; height: number };
let windowSize: Size = { width: 0, height: 0 };
const resizeListeners = new Set<(size: Size) => void>();

// Called from rn-linux on every `WindowEvent::Resized` — the one place
// outside React state that needs to reach into a live component tree, same
// pattern as reanimated's `__reanimatedTick`.
(globalThis as Record<string, unknown>).__scNotifyResize = function scNotifyResize(width: number, height: number): void {
  windowSize = { width, height };
  for (const listener of resizeListeners) listener(windowSize);
};

export function useWindowDimensions(): Size {
  const [size, setSize] = React.useState(windowSize);
  React.useEffect(() => {
    resizeListeners.add(setSize);
    return () => {
      resizeListeners.delete(setSize);
    };
  }, []);
  return size;
}

export const Dimensions = {
  get: (_what: 'window' | 'screen'): Size => windowSize,
  addEventListener: () => ({ remove: () => {} }),
};

// Bare (non-Reanimated) `Animated` — `@sc/ui` uses reanimated instead, this
// only exists so an import of the real RN legacy API doesn't crash. Values
// are static, `.start(cb)` resolves immediately rather than animating.
class AnimatedValue {
  constructor(private raw: number) {}
  setValue(v: number) {
    this.raw = v;
  }
  interpolate(_config: unknown) {
    return this;
  }
  __getValue() {
    return this.raw;
  }
}

export const Animated = {
  Value: AnimatedValue,
  View,
  Text,
  timing: (value: AnimatedValue, config: { toValue: number }) => ({
    start: (cb?: (result: { finished: boolean }) => void) => {
      value.setValue(config.toValue);
      cb?.({ finished: true });
    },
  }),
  spring: (value: AnimatedValue, config: { toValue: number }) => ({
    start: (cb?: (result: { finished: boolean }) => void) => {
      value.setValue(config.toValue);
      cb?.({ finished: true });
    },
  }),
};

export const Easing = {
  linear: (t: number) => t,
  ease: (t: number) => t * t * (3 - 2 * t),
  quad: (t: number) => t * t,
  cubic: (t: number) => t * t * t,
  bezier: () => (t: number) => t,
  in: (fn: (t: number) => number) => fn,
  out: (fn: (t: number) => number) => (t: number) => 1 - fn(1 - t),
  inOut: (fn: (t: number) => number) => (t: number) => (t < 0.5 ? fn(2 * t) / 2 : 1 - fn(2 * (1 - t)) / 2),
};
