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

Дальше — sc-rn TurboModule + живые экраны `@sc/ui` на бандлер-алиасе
`@shopify/react-native-skia`→`rnskia.tsx` (спайк 7). Windows-путь (RN-Windows +
Skia-порт) — после/параллельно, через `winbuild` (podman-windows, VS BuildTools
уже стоит).
