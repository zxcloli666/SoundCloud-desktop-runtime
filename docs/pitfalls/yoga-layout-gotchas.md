# Yoga: default `alignItems: stretch` surprises

Yoga's default cross-axis behaviour (`alignItems: stretch`) is correct and
wanted almost everywhere — it's exactly the two places it silently clamps
something that was supposed to overflow that caused real bugs.

## Horizontal `ScrollView`'s content wrapper couldn't overflow

**Симптом**: горизонтальный `ScrollView` физически не мог скроллиться —
контент никогда не был шире контейнера, сколько бы детей внутрь ни клали.

**Причина**: `ScrollView`'s внутренний контент-wrapper — column-direction
ребёнок (также column-direction) внешнего клиппинг-контейнера. Yoga's
дефолтный `alignItems: stretch` клэмпил ширину контент-wrapper'а до
ширины контейнера — ровно то единственное измерение, которое
горизонтальному скроллу нужно, чтобы естественно вырасти ЗА контейнер
(иначе физически нечего скроллить). Найдено вручную, гоняя живой рендер
(не тестом — тест написан ПОСЛЕ, чтобы закрепить фикс).

**Фикс**: `alignSelf: 'flex-start'` на контент-wrapper'е, но ТОЛЬКО когда
`horizontal` — вертикальный скролл, наоборот, корректно стретчится по
ширине контейнера, это желаемое поведение.

**Регресс-тест**: и движковый (`crates/js-host/src/tests/bundle_test.rs`,
на синтетическом `js/playground/`'s `OverflowCarousel` — фикстура
deliberately воспроизводит ТУ ЖЕ nesting-форму, не абстрактный "широкий
View в узком"), и на реальном `@sc/ui` (`e2e/tests/
real_ui_integration.rs`) — намеренное дублирование, см. `docs/pitfalls/
cross-repo-workspace-split.md`.

## Текстовые измерения маскируются default stretch в тестах

Не продовый баг — но грабля, на которую легко напороться, если писать
новый тест на реальные метрики текста (`crates/js-host/src/tests/
text_metrics.rs`).

**Симптом**: измеренная ширина текстового узла ВСЕГДА равна ширине
контейнера, независимо от длины строки/размера шрифта — выглядит как
будто `measure_func` вообще не вызывается.

**Причина**: контейнер без явной узкой ширины по умолчанию `alignItems:
stretch`'ит и `wrap`, и сам текстовый узел до полной ширины родителя
регардлесс того, что реально вернул `measure_text`. Реальный `@sc/ui`
никогда это не задевает, потому что его контейнеры всегда имеют свою
(обычно уже узкую) явную ширину.

**Фикс**: `alignItems: "flex-start"` на обоих уровнях контейнера в
тестовом хелпере `measured_text_width` — не продовый код, только тест.
