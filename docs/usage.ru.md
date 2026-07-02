# Гайд по использованию

Как подключить Desktop-Runtime из своего проекта: установка, минимальное
дерево, бандл под шимы, свои host-функции. (English version:
[`usage.md`](./usage.md).)

## 0. Кроссплатформенность — существующее RN-приложение на десктопе

Если у вас уже есть React Native приложение — iOS, Android, может
macOS через `react-native-macos` — использующее `@shopify/react-native-skia`
для кастомной отрисовки, Desktop-Runtime — это способ довести ТО ЖЕ
приложение до Windows и Linux, без форка и без переписывания компонентов:

1. Код компонентов (`View`/`Text`/`Pressable`/`Canvas`/`Group`/
   `useSharedValue`/...) не меняется. Он и так говорит только с публичным
   API `react-native`/`@shopify/react-native-skia`/`react-native-reanimated`
   — это и есть весь контракт.
2. На мобилках и macOS эти импорты резолвятся в настоящие нативные
   модули, как обычно.
3. На Windows/Linux сборка десктопного бандла резолвит те же импорты в
   шимы Desktop-Runtime (раздел 3 ниже) — одна esbuild `alias`-запись,
   тот же трюк, что `react-native-web` использует для браузерных сборок.
   Дереву компонентов не нужно знать, на какой платформе оно бежит.
4. Небольшой, десктоп-онли Rust-бинарь (раздел 2) открывает окно и
   хостит этот бандл — это новый код, который пишется один раз на
   приложение (это точка входа именно ВАШЕГО приложения на десктопе,
   Desktop-Runtime не может владеть ей за вас), не что-то, что форкается
   из существующего проекта.

Итог: одна кодовая база компонентов, пять платформ. Как именно
проверяется "твой Skia-код по-прежнему работает на Windows/Linux", а не
просто заявляется — [`compat/README.md`](../compat/README.md).

## 1. Установка

Rust-крейты и JS-пакет публикуются в собственные реестры этого репо — см.
[`registry.md`](./registry.md), почему не crates.io/npm напрямую.

**`.cargo/config.toml`** (в проекте или `~/.cargo/config.toml`):

```toml
[registries]
desktop-runtime = { index = "sparse+https://zxcloli666.github.io/SoundCloud-desktop-runtime/registry/" }
rusty-hermes-fork = { index = "sparse+https://zxcloli666.github.io/rusty_hermes/registry/" }
```

**`Cargo.toml`:**

```toml
[dependencies]
rn-linux = { version = "0.1.0", registry = "desktop-runtime" }     # Linux
rn-windows = { version = "0.1.0", registry = "desktop-runtime" }   # Windows
js-host = { version = "0.1.0", registry = "desktop-runtime" }
```

`rn-linux` и `rn-windows` дают идентичный API `run(RunConfig)`
(`rn-windows` — тонкий бинарный крейт над тем же платформо-агностичным
`rn_linux::run`, отдельного `rn_windows::run` нет — в самом движке ничего
Linux-специфичного нет); берите тот, что соответствует вашему `cargo
build --target`, либо разведите оба через `[target.'cfg(windows)'.
dependencies]` / `[target.'cfg(unix)'.dependencies]` в своём
`Cargo.toml`, если собираете под обе платформы из одного крейта.
`cargo add rn-linux --registry desktop-runtime` (или `rn-windows`).
Зависимость
`js-host` на `rusty_hermes` (биндинг Hermes, публикуется из реестра
своего собственного репо — см. [`registry.md`](./registry.md))
резолвится транзитивно — ничего дополнительно настраивать не нужно,
кроме двух реестров выше. Первая сборка компилирует Hermes из исходников
(~7-8 минут на Linux), дальше — как с любой другой зависимостью,
артефакт переиспользуется.

**JS-пакет** (шимы + host-config для react-reconciler):

```sh
npm config set @zxcloli666:registry https://npm.pkg.github.com
npm install @zxcloli666/desktop-runtime-js
```

## 2. Минимальное окно

`rn-linux::run` принимает `RunConfig` и никогда не возвращается — открывает
окно, эвалит бандл, крутит рендер-луп. Весь публичный API:

```rust
pub struct RunConfig {
    pub bundle_path: PathBuf,
    pub window_title: String,
    pub initial_size: (f64, f64),
    pub before_bundle_eval: Option<Box<dyn FnOnce(&js_host::Runtime) -> Result<(), String>>>,
}
```

```rust
fn main() {
    rn_linux::run(rn_linux::RunConfig {
        bundle_path: "dist/bundle.js".into(),
        window_title: "My App".to_string(),
        ..Default::default()
    });
}
```

`bundle_path` — путь к JS-бандлу, собранному под шимы движка (следующий
раздел); `rn-linux`/`rn-windows` не заботятся о его содержимом сверх того,
что он зовёт `react-reconciler`'s `updateContainer` против уже
зарегистрированного host-config'а движка.

## 3. Бандл под шимы

JS-код приложения импортирует `react-native` / `@shopify/react-native-skia`
/ `react-native-reanimated` совершенно обычным образом — esbuild `alias`
(тот же трюк, что использует `react-native-web`) резолвит их в шимы
движка на этапе сборки, а не в реальные нативные модули:

```js
// build.mjs
import * as esbuild from 'esbuild';

await esbuild.build({
  entryPoints: ['src/index.tsx'],
  bundle: true,
  outfile: 'dist/bundle.js',
  format: 'iife',       // у Hermes нет загрузчика модулей
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

Точка входа связывает `react-reconciler` с host-config движка и монтирует
дерево — это шаблонный код, нужный каждому потребителю один раз (движок
не владеет вашим React-деревом, вы владеете):

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

Более полный, реальный пример (нажимаемые тайлы, скроллящийся список,
`withTiming`-анимация) — `js/playground/src/index.tsx` в этом репо, тот
же паттерн, просто задействует больше поверхности шимов.

Цвета — тюплы `[r, g, b, a]` (float 0-1) или CSS-строки (`"#5a8cff"`,
`"rgba(0,0,0,0.35)"`) — работает и то, и другое, как в настоящем React
Native.

## 4. Свои host-функции

Если приложению нужны нативные возможности сверх рендера (auth, локальные
данные, платформенные API — что угодно, для чего приложение вообще
существует) — регистрируйте свои `js_host::hermes_op`-функции поверх 15
генерик-опов движка, через `RunConfig::before_bundle_eval`:

```rust
use js_host::hermes_op;

#[hermes_op(name = "__myGetVersion")]
fn get_version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}

fn main() {
    rn_linux::run(rn_linux::RunConfig {
        bundle_path: "dist/bundle.js".into(),
        before_bundle_eval: Some(Box::new(|rt| {
            get_version::register(rt).map_err(|e| e.to_string())
        })),
        ..Default::default()
    });
}
```

`before_bundle_eval` зовётся один раз, после регистрации генерик-опов
движка, но до чтения бандла — правильное место и для одноразовой
инициализации (открыть базу, прочитать конфиг — что нужно приложению до
того, как запустится хоть один JS). Асинхронные host-функции, которым
нельзя блокировать рендер-поток, могут использовать
`js_host::async_bridge::spawn_call` — полный реальный пример (host-функции
SoundCloud для auth/данных) — `examples/soundcloud/crates/sc-desktop-ops`
в этом репо, и `examples/soundcloud/crates/sc-desktop-example` — как это
подключается в `RunConfig`.

## 5. Совместимость и ограничения

Таблица совместимости — [в README](../README.md#compatibility): против
каких версий `react-native`/`@shopify/react-native-skia`/
`react-native-reanimated` проверены шимы. Реальные найденные и
исправленные баги — [`docs/pitfalls/`](./pitfalls/), стоит пробежаться,
если что-то ведёт себя неожиданно.

`rn-windows` собирается и рендерит на том же платформо-агностичном
движке, что и `rn-linux` (в `crates/` нет ничего платформо-специфичного,
кроме самих `winit`/`glutin`/`skia-safe`/`rusty_hermes`, а они уже
поддерживают Windows апстримом — см. `docs/pitfalls/
windows-msvc-build.md` за единственной реально Windows-специфичной
частью: проводкой самого Hermes через MSVC).
