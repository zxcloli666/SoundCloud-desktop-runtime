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
  js/                    react-reconciler host-config + тестовое дерево,
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

Дальше — JSI-биндинги под `@shopify/react-native-skia`/`reanimated` (спайк
5-6), затем sc-rn TurboModule и живые экраны `@sc/ui` (спайк 7). Windows-путь
(RN-Windows + Skia-порт) — после/параллельно, через `winbuild` (podman-windows,
VS BuildTools уже стоит).
