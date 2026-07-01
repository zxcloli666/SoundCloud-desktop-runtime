# SoundCloud-desktop-runtime — свой RN-рантайм для десктопа

Даёт `@sc/ui` (Core-репо `Core/ui`, RN+`@shopify/react-native-skia`) работать
нативно на **Windows и Linux** без webview — ни там, ни там `@shopify/
react-native-skia` не существует. ТЗ: `Core/docs/DESKTOP_RUNTIME_TZ.md`.

```
Desktop-Runtime/
  crates/skia-desktop/   GPU-Skia surface (skia-safe + winit/glutin), общий
                         для Linux-хоста и будущего Windows-бэкенда
  crates/js-host/        Hermes (rusty_hermes) + сцена-дерево (реальная Yoga)
                         + host-функции для JS; сюда же лягут JSI-биндинги
                         react-native-skia/reanimated
  crates/rn-linux/       бинарь: winit-окно + event loop, склеивает
                         skia-desktop и js-host
  js/                    react-reconciler host-config + react-native-skia-
                         совместимые примитивы (rnskia.tsx) + тестовое дерево,
                         esbuild → dist/bundle.js (`pnpm build`), которое
                         rn-linux грузит и eval'ит в Hermes
```

## Архитектура (решено 2026-07-01, не Fabric C++ от Meta)

`@sc/ui` не переписываем — экраны и компоненты одинаковые на всех платформах.
Но переиспользуем не всё дословно из ТЗ, а по компонентам:

- **Yoga** — настоящая библиотека as-is (не переписываем layout) → flexbox
  побитово идентичен мобиле.
- **Hermes** — тот же движок, что на Android/iOS (не Node/V8): меньше памяти,
  быстрее старт, консистентное поведение JS везде.
- **Skia** — `skia-safe` (тот же движок Google Skia, что и у react-native-skia).
- **Рендерер** — `react-reconciler` (JS, тот же примитив, на котором построен
  сам React Native, — НЕ web/DOM) с нашим host-config, а не вендоренный Fabric
  C++ (shadow-tree/scheduler Meta — самая недокументированная и хрупкая часть;
  даже RN-Windows не переиспользует её как библиотеку, пишет с нуля под себя).
- **Совместимость `@shopify/react-native-skia` / `react-native-reanimated`** —
  собственная нативная начинка под их настоящий JS-API, подключается на уровне
  резолвера бандлера (как `react-native-web` подменяет `react-native`), исходники
  `@sc/ui` не трогаются.

Почему так: даёт перф настоящего RN (Skia+GPU, никакого браузера), одинаковые
компоненты на всех платформах (Yoga/Skia/Hermes — те же библиотеки, что и на
мобиле), маленький объём нашего кода (не пишем Fabric-scheduler), стабильность
(риск — только в тонком клее, не в переизобретении internals Meta).

## Состояние

Спайки 2-4 (`Core/docs/DESKTOP_RUNTIME_TZ.md`) готовы и проверены на Linux —
настоящий `react-reconciler` дерево → Yoga layout → Skia GPU draw, без
Fabric C++:

- **Спайк 2**: winit-окно + skia-safe GPU-surface (OpenGL через glutin) рисует
  статичную сцену на Linux. `GlWindowSurface::snapshot_png()` — readback кадра
  в PNG для headless-проверки (компоузер может держать другое окно поверх,
  тогда экранный скриншот не покажет наше — используй снапшот).
- **Спайк 3**: Hermes встроен через `rusty_hermes` (собирается из исходников,
  ~8 мин; git-зависимость, не на crates.io). Тест `js-host`: JS вызывает
  host-функцию и получает результат обратно.
- **Спайк 4**: `js_host::scene::Scene` — дерево из `__scCreateView`/
  `__scCreateText`/`__scAppendChild`/`__scSetStyle` (JSON-пропы), геометрия —
  настоящая `yoga` (crate `yoga` = bschwind/yoga-rs, C++ Facebook Yoga,
  собирается системным g++/libstdc++, libc++-dev из README не понадобился).
- **Спайк 4b**: `js/` — настоящий `react-reconciler` (0.32, React 19) с нашим
  host-config вызывает те же host-функции вместо рукописного JS.
  **Грабли (все решены)**:
  - `skia_safe::Font::default()` даёт typeface с 0 глифов — реальный шрифт
    только через `FontMgr::default().legacy_make_typeface(None,
    FontStyle::default())` (резолвится в "Noto Sans", fontconfig видит 708
    семейств).
  - Hermes — bare-движок: нет `setTimeout`/`console`/`setImmediate` — шимы в
    `js-host/src/host.rs::PRELUDE_JS`. Hermes' Promise-полифилл сам зовёт
    `setImmediate` при `.then()` — без шима падает ДО пользовательского кода.
  - `react-reconciler` host-config API шире, чем в его README: он не
    упоминает `resolveUpdatePriority`/`getCurrentUpdatePriority`/
    `setCurrentUpdatePriority` (React 18+) и `maySuspendCommit`/
    `preloadInstance`/`startSuspendingCommit`/`suspendInstance`/
    `waitForCommitToBeReady` (React 19 "suspensey commit") — без них
    `undefined is not a function` в разных внутренних местах.
    `getRootHostContext`/`getChildHostContext` не должны возвращать `null`
    (`requiredContext()` считает это багом и шумит в консоль), хотя README
    прямо говорит, что можно.
  - Ошибки внутри `queueMicrotask`-колбэка — необработанный promise rejection,
    глушится молча без своего try/catch вокруг шима.
  - `ConcurrentRoot` (режим настоящего RN/Fabric) пока не завёлся — доходит до
    `updateContainer` без ошибок, но не планирует микротаск-коммит; рабочий
    путь сейчас — `LegacyRoot` + `Renderer.flushSyncFromReconciler(fn)`
    (публичное имя `flushSync`, в этой сборке экспортировано как
    `flushSyncFromReconciler`). **Разобраться с ConcurrentRoot до спайка 6** —
    reanimated, вероятно, расчитывает на конкурентную семантику скорее, чем
    "legacy" — не проблема качества кода, просто имя режима реконсилера.

- **Спайк 5**: `@shopify/react-native-skia` даёт `<Canvas>` как настоящий
  Fabric native-component + СВОЙ ВНУТРЕННИЙ `react-reconciler` (persistent-mode,
  `src/sksg/`), который строит `SkPicture` и отдаёт нативному view через
  `global.SkiaViewApi.setJsiProperty(nativeId, "picture", pic)` — это оправдано
  для их кросс-платформенности (Android/iOS/web), но мы владеем всем pipeline,
  так что **не повторяем два-реконсилера-архитектуру**: `Canvas`/`Group`/
  `Circle`/`Rect`/`RoundedRect`/`Blur`/`RadialGradient`/`LinearGradient`/`Box`/
  `BoxShadow` — просто новые типы узлов в НАШЕМ ОДНОМ дереве
  (`js-host/src/scene.rs`), без Yoga (как в реальной библиотеке — позиционируются
  в пиксельных координатах внутри ближайшего `Canvas`, не флексбоксом). Gradient/
  Blur-дети конфигурируют Paint родительской фигуры, а не рисуются сами.
  `js/src/rnskia.tsx` — JS-шим с тем же экспортом (+ `vec`/`rect`/`rrect`
  геометрия, `useClock` — заглушка до спайка 6). Полный набор — по грепу
  реального usage в `@sc/ui` (`Core/ui/src/primitives/{Atmosphere,Waveform,
  GlassSurface}.tsx` — только они используют Skia/Reanimated из всех 25 файлов
  пакета; императивного `Skia.*`-API/Path/Shader/Image/skia-текста НЕТ вообще).
  **Известное упрощение**: `color`-пропы — только `[r,g,b,a]`-массивы (как
  `backgroundColor`), НЕ CSS-строки (`"#fff"`/`"rgba(...)"`), которые принимает
  настоящий `Skia.Color()` — парсер CSS-цветов нужен до спайка 7 (реальный
  `@sc/ui` передаёт цвета из темы, вероятно строками).
  **Мелкая грабля**: esbuild's `jsx:'automatic'` вместе с `NODE_ENV=development`
  даёт `jsxDEV`-вызовы с безобидным `console.error` про "Static children should
  be an array" — не блокирует рендер, не нашёл чистого фикса без потери
  dev-ошибок react-reconciler, оставлено как есть.

- **Спайк 6**: reanimated-совместимый слой (`js/src/reanimated.tsx`) —
  `useSharedValue`/`useDerivedValue`/`useAnimatedStyle`/`withTiming`/
  `Animated.View`, ровно то, что использует `@sc/ui` (без `withSpring`/
  `runOnUI`/жестов — не нужны). **Осознанно не второй UI-runtime поток**
  (как настоящий reanimated) — мы владеем всем render loop однопоточно, так
  что "воркл" — просто функция, которую наш собственный per-frame tick
  (`__reanimatedTick`, зовётся из `rn-linux` перед каждым layout+draw)
  перезапускает заново; `useDerivedValue`/`useAnimatedStyle` пересчитываются
  КАЖДЫЙ раз без dependency-tracking (дёшево для десктопных объёмов анимаций).
  `SharedValue.value =` перехватывает результат `withTiming()` (тегированный
  дескриптор) и стартует интерполяцию; `Animated.View` регистрирует
  computed-style в реестре по instance-id через `ref`+`useEffect`, отдельно от
  обычного React-коммита (как и в настоящем reanimated). `rn-linux` теперь
  крутит непрерывный redraw-loop (`ControlFlow::Poll` + `request_redraw()` в
  конце каждого кадра) — нужно для анимаций, не только resize/input.
  Проверено `#[test]` (`reanimated_test`): ширина растёт 24→220 за реальные
  ~1.5с и точно оседает на target. **ConcurrentRoot всё ещё НЕ доделан**
  (см. спайк 4b) — низкий приоритет, в конце (см. task list, отдельно оценили:
  выигрыш в отзывчивости под нагрузкой, не в сыром перфе разового рендера).
  **Найден и НЕ связан с reanimated баг**: на тайловых WM (Hyprland игнорирует
  `with_inner_size`, реально даёт 847x1388 вместо 1024x640) GPU-скриншот
  (`snapshot_png()`) показывает фон только в верхних ~640px, хотя Yoga-layout
  и offscreen CPU-рендер (regression-тест `fills_arbitrary_aspect_ratio_test`)
  для ТОЙ ЖЕ сцены на 847x1388 — верны. Значит баг в GL-surface/resize-таймингах
  (`skia-desktop/gl_surface.rs`), не в Scene/Yoga/reconciler — расследовать
  отдельно (задача в task list), не блокирует остальное.

- **Спайк 7a**: настоящий `@sc/ui` (не копия!) рендерится через наш pipeline.
  `js/build.mjs` резолвит `react-native`/`@shopify/react-native-skia`/
  `react-native-reanimated` в наши шимы через esbuild `alias` (как
  `react-native-web` подменяет `react-native`) — `@sc/ui`'s `Atmosphere`/
  `ThemeProvider` импортированы напрямую из `@sc/ui` в `js/src/index.tsx`,
  исходники не тронуты. `@sc/ui` добавлен в `js/package.json` (registry-semver
  `^0.1.0`) + в корневой `pnpm-workspace.yaml` (`Desktop-Runtime/js`) для
  локальной линковки — та же схема, что у `Desktop/app`/`Mobile/app`.
  **Шимы расширены НАМНОГО шире текущего usage `@sc/ui`** (юзер: покрыть
  заранее, не подбирать по одной функции): `react-native.tsx` — View/Text/
  Image/Pressable/Touchable*/ScrollView/FlatList/SectionList/SafeAreaView/
  ActivityIndicator/Switch/TextInput/Modal/StatusBar/Alert/Keyboard/AppState/
  BackHandler/Linking/StyleSheet/Platform/PixelRatio/Dimensions/
  useWindowDimensions (реактивен на resize через `__scNotifyResize`, зовётся
  из `rn-linux` при `WindowEvent::Resized`)/bare `Animated`/`Easing`.
  `rnskia.tsx` — + Path/Text/Image/Paint/Shader/ColorMatrix/BackdropBlur/
  BackdropFilter/Mask (эффекты деградируют грациозно: монтируются и лежат
  верно, эффект пока не применяется)/useImage/useFont/императивный `Skia.*`
  facade. `reanimated.tsx` — + withSpring (аналитический demped-oscillator)/
  withDecay/withSequence/withRepeat/runOnUI/runOnJS (both = "просто вызови
  сейчас", один поток)/useAnimatedReaction/useFrameCallback/
  useAnimatedProps/createAnimatedComponent/interpolate/interpolateColor/
  cancelAnimation/Extrapolation. **Важно**: react-native-skia позволяет ЛЮБОЙ
  проп быть Reanimated `SharedValue` (не только `style`, как в `Animated.View`)
  — `@sc/ui`'s idle-дрейф передаёт `transform={useDerivedValue(...)}` прямо в
  `<Group>`. `hostConfig.ts` детектит shared-value-like пропы (duck-typing
  `'value' in v`) и регистрирует узел на пере-сериализацию КАЖДЫЙ
  `__reanimatedTick`, не только при коммите (`applySkProps`/
  `__scRefreshAnimatedSkProps`).
  Rust-сторона (`scene.rs`) тоже расширена широко: `StyleInput` — position/
  left/right/top/bottom, flexWrap/alignSelf/alignContent/gap/rowGap/
  columnGap, aspectRatio/display, per-corner border-radius, borderWidth/
  borderColor, opacity (через save_layer), overflow:hidden (clip),
  percent-размеры (`Dimension` enum Point|Percent). Цвета — полноценный
  парсер CSS (`parse_color`): hex #rgb/#rrggbb/#rrggbbaa, rgb()/rgba(),
  `transparent`, именованные цвета — не только `[r,g,b,a]`-массивы (снимает
  упрощение спайка 5).

  **⚠️ КРУПНАЯ НАХОДКА — движковый баг Hermes**, не в нашем коде: `for (let
  key of ...)` цикл, где тело создаёт замыкание через `Object.defineProperty`,
  НЕ даёт свежий `key`-биндинг на каждой итерации — все геттеры видят
  ПОСЛЕДНИЙ key. Это ТОЧНАЯ форма esbuild-овского helper'а `__copyProps`
  (CJS→ESM interop для именованных импортов, `import {createContext, useMemo}
  from 'react'` — ThemeProvider.tsx первым в проекте использует именно эти
  два, до сих пор обходились `createElement`/`useState`/`useRef`/`forwardRef`)
  — ЛЮБОЕ свойство CJS-модуля (react.createContext, useMemo, ...) начинает
  возвращать ПОСЛЕДНЕЕ enumerable-свойство модуля (`version`, строка "19.2.3")
  → `'19.2.3' is not a function`. Подтверждено изолированным репродюсом (ловится
  и вне бандла, тест `hermes_for_of_let_closure_bug_test`) и Node (та же сборка
  ИСПОЛНЯЕТСЯ ВЕРНО в V8 — баг специфичен для Hermes). ES5-таргет esbuild не
  подошёл (проект не транспилится целиком до ES5 — const/деструктуризация).
  **Фикс — постпроцессинг бандла** (`js/build.mjs`, после `esbuild.build()`):
  regex-патч меняет `for...of`+`let` на `.forEach(function(key) {...})`
  (параметр функции — Hermes с ним работает верно, в отличие от let-в-цикле).
  Патч кидает ошибку сборки, если esbuild когда-нибудь поменяет форму
  helper'а (нужно будет обновить regex).

Дальше — sc-rn TurboModule (живые данные, оставшаяся часть спайка 7). Windows-
путь (RN-Windows + Skia-порт, спайк 1) — через `winbuild` (podman-windows, VS
BuildTools уже стоит).
