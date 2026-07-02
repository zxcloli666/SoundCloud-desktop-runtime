# react-reconciler: недокументированный host-config surface

`react-reconciler`'s README сильно отстаёт от того, что реально требуют
React 18/19 внутри — три отдельные находки, все всплыли на реальном
монтаже/апдейте дерева, не из документации.

## Host-config API шире README

**Симптом**: `undefined is not a function` в разных внутренних местах
реконсилера, `getRootHostContext`/`getChildHostContext`, возвращающие
`null`, шумят в консоль как баг.

**Причина**: README не упоминает `resolveUpdatePriority`/
`getCurrentUpdatePriority`/`setCurrentUpdatePriority` (нужны с React 18+)
и `maySuspendCommit`/`preloadInstance`/`startSuspendingCommit`/
`suspendInstance`/`waitForCommitToBeReady` (React 19 "suspensey commit").
`getRootHostContext`/`getChildHostContext` не должны возвращать `null`
вопреки README — `requiredContext()` внутри реконсилера считает это багом.

**Фикс**: реализовать весь фактический surface в `js/src/hostConfig.ts`,
не только задокументированный в README подмножество.

## Синхронные `setTimeout`/`setImmediate`-шимы ломают апдейты после монтажа

Самая коварная находка сессии — не баг Hermes, наш собственный.

**Симптом**: `setState` внутри `useEffect`/`.then()` вызывался
(залогировано), но экран не менялся. Никакого краша — просто апдейт
пропадал молча.

**Причина**: `setTimeout`/`setImmediate`-шимы (тогда в `host.rs::
PRELUDE_JS`) звали колбэк СИНХРОННО-ИНЛАЙН ("рендерим статичное дерево,
ждать нечего" — казалось разумным упрощением на момент написания). Но
react-reconciler's `scheduler`-пакет использует ИМЕННО `setImmediate`/
`setTimeout` как примитив "выйти на свежий стек" — чтобы запланировать
коммит ИЗНУТРИ уже идущего коммита (ровно кейс `useEffect`→`setState`) без
реентрантного вызова реконсилера. Синхронный шим это ломает: колбэк
вызывается ТУТ ЖЕ, реентерит `performWorkOnRoot`, попадает на собственный
guard React'а → кидает `Error: Should not already be working` — молча
проглатывается `queueMicrotask`'s try/catch (виден только как
`console.error`, не краш). Это же объясняло отдельно наблюдавшуюся
загадку "`ConcurrentRoot` доходит до `updateContainer`, но не коммитит" —
один и тот же корень, не два разных.

**Фикс** (`js-host/src/host.rs::PRELUDE_JS`): таймеры больше не зовут
колбэк инлайн — кладутся в очередь (`Map` id→{fireAt, fn, intervalMs},
время — `Date.now()`, доступен в голом Hermes без шима), дренится
`__scDrainTimers()`, которую рендер-луп (`rn-linux::run`) зовёт КАЖДЫЙ
кадр свежим вызовом (не вложенным ни в какой другой `eval`) — рядом с
reanimated-тиком и `async_bridge::deliver()`, ДО `drain_microtasks()`.
Побочный эффект: `ConcurrentRoot` (настоящий режим RN) после фикса просто
заработал сам по себе, без отдельной работы над ним.

## `commitTextUpdate` не был реализован

**Симптом**: не проявлялось до первого экрана, где текст МЕНЯЕТСЯ после
монтажа (счётчики, статусы, живые данные) — до этого момента реконсилер
никогда не звал `commitTextUpdate`, только `createTextInstance` при
первом монтаже.

**Причина**: заглушка `throw new Error('text updates not supported yet')`
осталась нетронутой, поскольку до первого live-текста ничего её не звало.

**Фикс**: `Scene::set_text` (`scene.rs`, пере-считает placeholder-ширину
под новую строку) + `__scSetText` host-функция + `hostConfig.ts`'s
`commitTextUpdate` реально её зовёт.
