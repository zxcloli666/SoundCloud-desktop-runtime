# Windows/MSVC: пять апстримных багов в `rusty_hermes`/`libhermes-sys`

Все пять — реальные апстримные баги в `libhermes-sys/build.rs` (не в
нашем коде), найдены и исправлены в форке
[`zxcloli666/rusty_hermes`](https://github.com/zxcloli666/rusty_hermes)
(ветка `windows-cmake-generator-fix`), апстримный PR:
[`rust-hermes/rusty_hermes#7`](https://github.com/rust-hermes/rusty_hermes/pull/7).
Linux/macOS не тронуты — либо отдельная `cfg!`-ветка, либо изменение
нейтрально для POSIX.

1. **Генератор Ninja терялся**: `-G Ninja` передавался через
   `configure_arg` (сырую строку), а не через `Config::generator("Ninja")`
   — `cmake`-крейт распознаёт Ninja ТОЛЬКО через своё поле `generator`; на
   MSVC-таргете он иначе считает генератором Visual Studio и добавляет
   `-Thost=x64 -Ax64`, конфликтующие с реально переданным `-G Ninja`
   ("Generator Ninja does not support platform specification"). Фикс —
   `.generator("Ninja")`.

2. **GCC/Clang-флаги молча игнорировались `cl.exe`**: `binding.cc`'s
   `cc::Build` звал `.flag("-std=c++17")`/`.flag("-fexceptions")`/
   `.flag("-frtti")` — `cl.exe` молча игнорирует незнакомые `-`-флаги (не
   ошибка, просто ничего не делает) → компилируется в до-C++17 режиме →
   падает на structured bindings. Фикс — `.flag_if_supported(...)` с
   ОБЕИМИ спеллинг-парами (POSIX и MSVC `/std:c++17`/`/EHsc`/`/GR`).

3. **Дискавери либов и system-lib fallback были POSIX-only**: фильтр
   собранных статик-либов смотрел только на `*.a` (на MSVC архивы
   `*.lib`) — ничего не находилось; параллельно system-lib fallback
   слепо добавлял `stdc++`/`icuuc`/`icui18n`/`icudata` для "не-macOS"
   (Linux-only имена, на Windows таких либ нет) — маскировал баг #4
   (линкер падал на несуществующий `stdc++.lib` раньше, чем дошёл бы до
   реальной проблемы с Hermes-символами). Фикс — платформенный выбор
   расширения (`.lib` vs `.a`) + system libs разведены на три ветки
   (`macos`/`linux`/остальное).

4. **Debug/Release CRT рассинхрон**: после фикса #3 линкер дошёл до
   Hermes-символов и упал на десятки `unresolved external symbol
   __imp__calloc_dbg` (и подобных). Причина: `cmake`-крейт сам инферит
   `CMAKE_BUILD_TYPE` из профиля Cargo ("Debug" для обычного `cargo
   build") — под MSVC это тянет CMake-овские дефолтные `/MDd`-флаги
   (debug CRT) для объектников Hermes, а Rust'овский финальный линк
   ВСЕГДА ждёт релизный CRT (`/MD`), вне зависимости от `--release`.
   Фикс — `.profile("Release")` безусловно (вендоренная C++ VM,
   собранная неоптимизированной под дев-профиль, не даёт ничего полезного
   — исходники не правим — и просто медленнее в рантайме).

5. **ICU/Winmm импорт-либы**: убрав #4, ещё один реальный набор
   недостающих символов: `unorm2_*`/`ucol_*`/`udat_*`/`u_strTo*` (ICU) и
   `timeBeginPeriod`/`timeEndPeriod` (Winmm). CMake-лог "Using Windows 10
   built-in ICU" означает лишь, что `PlatformUnicodeICU.cpp` зовёт ТЕ ЖЕ
   точки входа, что и ICU4C, но через системный `icu.dll` — линковку
   этого сам Hermes не встраивает в `hermesvm_a.lib`; имя импорт-либы на
   Windows — `icu` (не POSIX-овые `icuuc`/`icui18n`/`icudata`). Фикс —
   `cargo:rustc-link-lib=icu` + `=winmm` в ветке Windows.

## Побочная находка окружения

Python на тестовой Windows-машине был установлен только `py`-лаунчером
без самого интерпретатора (`python-installer.exe /quiet` тихо не долетал
до компонента интерпретатора при первой попытке) — переустановлен с
логом, реальный `python.exe 3.12.7` подтверждён напрямую.

## Итог

Тестовый крейт (`rusty_hermes` из форка) собирается под MSVC (`cargo
build`, ~14.5 минут первой полной компиляции Hermes через MSVC/Ninja) и
реально выполняет JS. Архитектурный блокер снят — остаётся рутинный
скаффолд `rn-windows`-бинаря (winit/glutin/skia-safe уже поддерживают
Windows апстримом теми же крейтами, что и `rn-linux`) — не сделан, но
больше не заблокирован технически.
