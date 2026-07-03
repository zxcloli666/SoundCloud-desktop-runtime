# Гайд по использованию

Пошагово: берёшь своё существующее React Native приложение (iOS/Android,
может macOS через `react-native-macos`) и добавляешь Windows + Linux, из
той же кодовой базы компонентов, в одном репо. (English version:
[`usage.md`](./usage.md). Зачем это вообще нужно: [README.md](../README.md).)

## Что получится в итоге

```
my-app/                    твоё существующее RN-приложение — БЕЗ ИЗМЕНЕНИЙ
  src/                     общие компоненты — те же файлы на всех платформах
  index.js  ios/  android/  macos/  package.json    не тронуты, никакого Cargo

  desktop/                 НОВОЕ — единственная папка, знающая про Desktop-Runtime
    js/
      package.json
      build.mjs             собирает ../../src/index.tsx -> dist/bundle.js
    Cargo.toml              свой собственный [workspace]
    windows/                зависит от rn-windows
      Cargo.toml
      src/main.rs
    linux/                  зависит от rn-linux
      Cargo.toml
      src/main.rs
```

`ios/`, `android/`, `macos/` не меняются и никогда не видят Cargo. Всё
дальше происходит внутри `desktop/`.

## Шаг 1 — Добавить два реестра

Rust-крейты и JS-пакет хостятся в собственных реестрах этого репо (почему
не crates.io/npm: [`registry.md`](./registry.md)). Добавить оба, один
раз, в `~/.cargo/config.toml` (или `desktop/.cargo/config.toml` для
проектной настройки):

```toml
[registries]
desktop-runtime = { index = "sparse+https://zxcloli666.github.io/SoundCloud-desktop-runtime/registry/" }
rusty-hermes-fork = { index = "sparse+https://zxcloli666.github.io/rusty_hermes/registry/" }
```

## Шаг 2 — Создать `desktop/`

```sh
mkdir -p desktop/js/src desktop/windows/src desktop/linux/src
```

`desktop/Cargo.toml` — собственный workspace, чтобы `cargo build` где-то
ещё в репо никогда его не задел:

```toml
[workspace]
members = ["windows", "linux"]
resolver = "2"
```

## Шаг 3 — Добавить Rust-крейт под каждую платформу

`desktop/windows/Cargo.toml`:

```toml
[package]
name = "my-app-windows"
version = "0.1.0"
edition = "2021"

[dependencies]
rn-windows = { version = "0.1.0", registry = "desktop-runtime" }
```

`desktop/linux/Cargo.toml` — идентично, кроме:

```toml
[dependencies]
rn-linux = { version = "0.1.0", registry = "desktop-runtime" }
```

Каждый бинарь зависит только от своего крейта — сборка `desktop/linux`
никогда не подтягивает `rn-windows`, и наоборот.

## Шаг 4 — Написать `main.rs`

Одинаковое содержимое в `desktop/windows/src/main.rs` и
`desktop/linux/src/main.rs` (поменять `rn_windows`/`rn_linux` на крейт из
шага 3 — оба дают идентичный `run(RunConfig)`):

```rust
fn main() {
    rn_linux::run(rn_linux::RunConfig {
        bundle_path: "../js/dist/bundle.js".into(),
        window_title: "My App".to_string(),
        ..Default::default()
    });
}
```

Весь `RunConfig`:

```rust
pub struct RunConfig {
    pub bundle_path: PathBuf,
    pub window_title: String,
    pub initial_size: (f64, f64),
    pub before_bundle_eval: Option<Box<dyn FnOnce(&js_host::Runtime) -> Result<(), String>>>,
}
```

## Шаг 5 — Установить JS-пакет шимов

```sh
cd desktop/js
npm config set @zxcloli666:registry https://npm.pkg.github.com
npm install @zxcloli666/desktop-runtime-js esbuild
```

## Шаг 6 — Написать `build.mjs`

Вот и вся магия: обычные импорты `react-native`/
`@shopify/react-native-skia`/`react-native-reanimated` в твоём
приложении резолвятся в шимы Desktop-Runtime вместо настоящих нативных
модулей — на этапе сборки. Код компонентов ничего Desktop-Runtime-
специфичного не импортирует.

```js
// desktop/js/build.mjs
import * as esbuild from 'esbuild';

await esbuild.build({
  entryPoints: ['../../src/index.tsx'],
  bundle: true,
  outfile: 'dist/bundle.js',
  format: 'iife',           // у Hermes нет загрузчика модулей
  platform: 'neutral',
  mainFields: ['main'],
  target: 'es2020',
  jsx: 'automatic',
  define: { 'process.env.NODE_ENV': '"development"' },
  alias: {
    'react-native': 'node_modules/@zxcloli666/desktop-runtime-js/src/react-native.tsx',
    '@shopify/react-native-skia': 'node_modules/@zxcloli666/desktop-runtime-js/src/rnskia.tsx',
    'react-native-reanimated': 'node_modules/@zxcloli666/desktop-runtime-js/src/reanimated.tsx',
  },
});
```

## Шаг 7 — Подключить точку входа

`src/index.tsx` нуждается в небольшом шаблонном коде, чтобы отдать своё
React-дерево движку (одинаково для каждого потребителя, движок не может
сделать это за вас — он не владеет вашим деревом):

```tsx
import React from 'react';
import Reconciler from 'react-reconciler';
import { ConcurrentRoot } from 'react-reconciler/constants';
import { hostConfig } from '@zxcloli666/desktop-runtime-js/src/hostConfig';
import { Text, View } from 'react-native';

const Renderer = Reconciler(hostConfig);

function App() {
  return (
    <View style={{ backgroundColor: [0.04, 0.05, 0.08, 1.0] }}>
      <Text style={{ color: [1, 1, 1, 1], margin: 16 }}>Привет, Desktop-Runtime</Text>
    </View>
  );
}

const root = Renderer.createContainer(
  { rootId: null }, ConcurrentRoot, null, false, null, '',
  (e) => { throw e; }, (e) => { throw e; }, (e) => { throw e; }, null,
);
Renderer.updateContainer(<App />, root, null, null);
```

Цвета — тюплы `[r, g, b, a]` (float 0-1) или CSS-строки (`"#5a8cff"`,
`"rgba(0,0,0,0.35)"`) — работает и то, и другое, как в настоящем React
Native.

Более полный пример (нажимаемые тайлы, скроллящийся список,
`withTiming`-анимация) на том же паттерне: `js/playground/src/index.tsx`
в этом репо.

## Шаг 8 — Собрать и запустить

```sh
cd desktop/js && pnpm install && node build.mjs   # -> dist/bundle.js

# Windows:
cd desktop/windows && cargo run --release

# Linux:
cd desktop/linux && cargo run --release
```

Первая сборка компилирует Hermes из исходников (~7-8 минут), дальше —
как с любой другой зависимостью. Один и тот же `dist/bundle.js`
запускается на обеих платформах — пересобирать под каждую отдельно не
нужно.

## Шаг 9 (опционально) — Свои нативные функции

Если приложению нужны нативные возможности сверх рендера (auth,
локальные данные, что угодно) — регистрируйте свою host-функцию поверх
16 встроенных опов движка:

```rust
use js_host::hermes_op;

#[hermes_op(name = "__myGetVersion")]
fn get_version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}

fn main() {
    rn_linux::run(rn_linux::RunConfig {
        bundle_path: "../js/dist/bundle.js".into(),
        before_bundle_eval: Some(Box::new(|rt| {
            get_version::register(rt).map_err(|e| e.to_string())
        })),
        ..Default::default()
    });
}
```

`before_bundle_eval` зовётся один раз, до чтения бандла — правильное
место и для другой одноразовой инициализации (открыть базу, прочитать
конфиг). Для асинхронного, что не должно блокировать рендер-поток —
`js_host::async_bridge::spawn_call`, полный реальный пример —
`examples/soundcloud/crates/sc-desktop-ops` в этом репо, и
`examples/soundcloud/crates/sc-desktop-example` — как это подключается в
`RunConfig`.

## Справочно

- **Совместимость**: [таблица в README](../README.md#compatibility) —
  против каких версий `react-native`/`@shopify/react-native-skia`/
  `react-native-reanimated` проверены шимы.
- **Уже найденные и исправленные баги**: [`docs/pitfalls/`](./pitfalls/)
  — стоит пробежаться, если что-то ведёт себя неожиданно.
- **Про Windows отдельно**: `rn-windows` работает на том же движке, что
  и `rn-linux` — в `crates/` нет ничего платформо-специфичного, кроме
  самих `winit`/`glutin`/`skia-safe`/`rusty_hermes`, а они уже
  поддерживают Windows апстримом. Единственная реально Windows-
  специфичная часть — проводка самого Hermes через MSVC — описана в
  [`docs/pitfalls/windows-msvc-build.md`](./pitfalls/windows-msvc-build.md).
