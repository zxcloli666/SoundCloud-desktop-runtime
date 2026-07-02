# SoundCloud-desktop-runtime — свой RN-рантайм для десктопа

Даёт React Native компонентам (реальным, включая `@sc/ui`) работать
нативно на **Windows и Linux** без webview — ни там, ни там
`@shopify/react-native-skia` не существует как готовое решение. ТЗ:
`Core/docs/DESKTOP_RUNTIME_TZ.md`.

Список находок/багов, решённых по пути (движковые баги Hermes, Yoga
gotchas, гонки с react-reconciler'ом и т.д.) — вынесен отдельно, чтобы не
мешать понять текущую архитектуру: **[`docs/pitfalls/INDEX.md`](docs/pitfalls/INDEX.md)**.

## Архитектура: реальные библиотеки, не вендоренный Fabric C++

`@sc/ui` не переписывается — экраны и компоненты одинаковые на всех
платформах. Но не копируется собственный движок целиком из спецификации
Meta — переиспользуется по компонентам:

- **Yoga** — настоящая библиотека as-is (`crate yoga` = bschwind/yoga-rs,
  C++ Facebook Yoga) → flexbox побитово идентичен мобиле.
- **Hermes** — тот же движок, что на Android/iOS (не Node/V8): меньше
  памяти, быстрее старт, консистентное поведение JS везде. Встроен через
  `rusty_hermes` (собирается из исходников).
- **Skia** — `skia-safe` (тот же движок Google Skia, что и у
  react-native-skia).
- **Рендерер** — настоящий `react-reconciler` (тот же примитив, на
  котором построен сам React Native — НЕ web/DOM) со своим host-config, а
  не вендоренный Fabric C++ (shadow-tree/scheduler Meta — самая
  недокументированная и хрупкая часть; даже RN-Windows не переиспользует
  её как библиотеку, пишет с нуля под себя).
- **Совместимость `@shopify/react-native-skia` / `react-native-reanimated`** —
  собственная реализация под их настоящий JS-API, подключается на уровне
  резолвера бандлера (как `react-native-web` подменяет `react-native`
  через esbuild `alias`) — исходники потребителя (`@sc/ui` и любой
  другой) не трогаются.

Почему так: перф настоящего RN (Skia+GPU, никакого браузера), одинаковые
компоненты на всех платформах (Yoga/Skia/Hermes — те же библиотеки, что и
на мобиле), маленький объём собственного кода (не пишется
Fabric-scheduler), стабильность (риск — только в тонком клее, не в
переизобретении internals Meta).

## Структура репозитория

```
Desktop-Runtime/          ДВИЖОК — корневой Cargo/pnpm workspace, ZERO
                          SC-специфичных или иных внешних зависимостей
  crates/skia-desktop/   GPU-Skia surface (skia-safe + winit/glutin)
  crates/js-host/        Hermes-рантайм + сцена-дерево (реальная Yoga) +
                         16 генерик host-функций для JS
  crates/rn-linux/       lib.rs (RunConfig/run — публичная библиотека,
                         платформо-агностичная) + тонкий main.rs: winit-окно
                         + event loop
  crates/rn-windows/     тот же движок (rn_linux::run) под своим
                         `rn-windows.exe` — свой main.rs, ноль
                         платформенного кода: winit/glutin/skia-safe уже
                         кроссплатформенны сами по себе
  js/                    react-reconciler host-config + react-native/
                         react-native-skia/reanimated-совместимые шимы +
                         свой zero-dep playground (js/playground/) —
                         именно его грузит rn-linux по умолчанию

examples/soundcloud/      как этим движком пользуется сам SoundCloud —
                          ОТДЕЛЬНЫЙ вложенный Cargo/pnpm workspace, требует
                          Core-репо рядом (сиблинг-чекаут)
  crates/sc-desktop-ops/ плагин: SoundCloud-специфичные host-функции
                         (auth/данные через sc-rn) — единственное место во
                         всём репо, где реально виден sc-rn
  crates/sc-desktop-example/  тонкий бинарь: rn_linux::run() +
                              sc-desktop-ops::install() через
                              RunConfig::before_bundle_eval
  js/                    настоящий `@sc/ui`-демо — единственный
                         package.json во всём репо с `@sc/ui`-зависимостью

e2e/                       интеграционные тесты — ОТДЕЛЬНЫЙ Cargo
                          workspace, требует Core + собранный
                          examples/soundcloud/js; реальный sc-rn +
                          реальный @sc/ui end-to-end

docs/pitfalls/             находки/баги по пути, топик на файл
compat/                    структурная сверка шимов против реальных
                          react-native/react-native-skia/reanimated типов
.github/workflows/         compat-check.yml (cron) + publish.yml
```

**Инвариант**: `crates/`+`js/` (корень) собираются и тестируются с нуля,
не имея на диске ничего, кроме этого репозитория. `examples/` и `e2e/` —
каждый свой `[workspace]`, физически отделены (Cargo при обходе вверх
останавливается на ближайшем `[workspace]`-столе, до корневого не
доходит; корневой `Cargo.toml` вдобавок явно их `exclude`-ит).

## Сборка / запуск / тесты

Движок (ничего кроме этого репо не нужно):
```
cd js && pnpm install && pnpm build   # → js/dist/playground.js
cargo run -p rn-linux                 # рендерит playground (Linux)
cargo run -p rn-windows               # тот же playground, Windows-бинарь
cargo test --workspace                # 20+ тестов, полностью zero-dep
```

Как SoundCloud (нужен `Core` рядом, сиблингом этому репо):
```
cd examples/soundcloud/js && pnpm install && pnpm build
cd examples/soundcloud && cargo run -p sc-desktop-example
cd e2e && cargo test   # реальный sc-rn + реальный @sc/ui
```

## Состояние

- **Linux**: готово и проверено — реальный `react-reconciler` дерево →
  Yoga layout → Skia GPU draw, инпут (клик/скролл), живые данные через
  `sc-rn`, полное покрытие шимов против реального usage `@sc/ui`.
- **Windows**: архитектурный блокер (сборка `rusty_hermes`/
  `libhermes-sys` под MSVC) снят — реальный Hermes собирается и выполняет
  JS на Windows (см. `docs/pitfalls/windows-msvc-build.md`, проверено на
  живой Windows-машине). `crates/rn-windows/` собран и подтверждён на
  этом же Linux-хосте (тот же кроссплатформенный winit/glutin/skia-safe
  стек, что у `rn-linux`, — ни строчки платформенного Rust-кода в самом
  движке) — рендерит playground один в один с `rn-linux`. Сам прогон
  `rn-windows.exe` на реальной Windows-машине (а не просто сборка Hermes
  под MSVC) в этом заходе не переделывался — это единственное, что
  формально не переподтверждено живым запуском на Windows.

## Публикация

Пакеты (движковые крейты + `js/`-пакет) публикуются через GitHub Actions
на GitHub-инфраструктуру — не crates.io/npm напрямую (см. `docs/
pitfalls/` за деталями, если появятся). Подробная инструкция для
потребителей — `docs/usage.md` (англ) / `docs/usage.ru.md` (рус).
