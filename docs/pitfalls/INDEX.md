# Грабли и находки

Каждая — реальная проблема, найденная и решённая при постройке этого
рантайма, с причиной и фиксом. Не changelog — если баг был закрыт до
того, как повлиял на архитектуру, здесь его нет; если влияет на то, как
кто-то дальше будет работать с этим кодом или в похожую яму упадёт
снова — здесь есть.

- [Hermes engine bugs](hermes-engine-bugs.md) — движковый баг Hermes с
  `for...of`+`let`-замыканиями, ломает esbuild's CJS-interop helper
- [react-reconciler: недокументированный surface](react-reconciler-integration.md) —
  недостающие host-config методы, гонка `setTimeout`/scheduler'а
  (пропадающие апдейты), нереализованный `commitTextUpdate`
- [Yoga: `alignItems: stretch` сюрпризы](yoga-layout-gotchas.md) —
  горизонтальный `ScrollView` не мог скроллиться; маскировка измерений
  текста в тестах
- [Стилевой пайплайн JS↔Rust](shim-style-plumbing-bugs.md) — array-form
  `style` не флэттенился, `createAnimatedComponent`'s спред-баг,
  TS wildcard ломал контекстную типизацию колбэков
- [GL-поверхность: гонка ресайза на Wayland](gpu-surface-resize.md) —
  `window.inner_size()` нельзя доверять внутри `WindowEvent::Resized`
- [Тесты: process-global каналы](test-concurrency-flakes.md) — два
  отдельных флейка от одного и того же класса бага (дренится-целиком
  канал, шарится между потоками тестов)
- [Windows/MSVC: 5 багов в `rusty_hermes`](windows-msvc-build.md) —
  апстримные баги в `libhermes-sys/build.rs`, все исправлены в форке
- [Разделение движка и примера](cross-repo-workspace-split.md) —
  `#[hermes_op]`-макро не re-export-aware, нужен ре-экспорт модуля целиком
- [Мелкие грабли](misc-gotchas.md) — 0-глифовый дефолтный шрифт Skia,
  безобидный esbuild-шум, снятые упрощения
