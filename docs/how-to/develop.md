# Собрать и проверить Lab Colors

```sh
cargo test --workspace --locked
```

Версия Rust указана в [Cargo.toml](../../Cargo.toml). Команды сборки и проверки
npm-пакета находятся в [package.json](../../packages/colors/package.json).

```sh
npm --prefix packages/colors run build
```

Сборка требует rustup, Rust из `rust-toolchain.toml`, `wasm-pack` из
[`bench/wasm.json`](../../packages/colors/bench/wasm.json), Node.js и `tar` в PATH.
Она отключает только встроенный optimizer `wasm-pack`, затем выполняет
официальный Binaryen Node с теми же release, SHA-256 и флагами, что указаны
в `bench/wasm.json`. Оптимизированный WASM проверяется перед завершением.
Первый запуск скачивает архив с GitHub; следующие используют проверяемый
по SHA-256 пользовательский кэш. Для уже скачанного архива задайте
`LABCOLORS_BINARYEN_ARCHIVE`: его байты проходят ту же проверку.

Локальная сборка и CI вызывают `scripts/build-wasm.mjs`. Базовый WASM release
профиль задан в `bench/wasm.json` → `recipe.cargoProfile` и передаётся Cargo
через `--config`. Native release остаётся в корневом `Cargo.toml`.
Скрипт выбирает закреплённый `rustc` через rustup, задаёт `CARGO_INCREMENTAL`
из рецепта и передаёт remap через `CARGO_ENCODED_RUSTFLAGS`. Cargo использует
эти флаги вместо остальных источников rustflags. `--config` перекрывает
одноимённые настройки профиля из среды и конфигурационных файлов. Настройки
параллелизма, кэшей, offline, proxy и compiler wrappers продолжают работать.

`wasm-pack` и последующий `cargo metadata` запускаются из каталога WASM-крейта,
откуда `wasm-pack 0.13.1` вызывает `cargo build`. Поэтому обе команды читают
одну иерархию Cargo config, включая конфигурацию самого крейта.

Для сравнения двух ревизий передайте путь исходников единому скрипту текущей
ревизии, например `node scripts/build-wasm.mjs /path/to/base-checkout`.
Профиль и инструменты берутся из текущего рецепта, исходники и выходной `pkg` —
из переданного checkout. Каждый успешный build сохраняет в `pkg/wasm-build.json`
профиль из сообщений Cargo, SHA-256 WASM и входов сборки. Этот файл используется
при проверке воспроизводимости в CI и исключён из списка файлов npm-пакета.

Проверка сообщений Cargo сверяет `opt_level`, `debuginfo`, `debug_assertions`,
`overflow_checks` и `test` у WASM-крейта и его target-зависимостей. Значение
`debuginfo: null` соответствует уровню 0. Package overrides, меняющие эти
поля, завершают сборку ошибкой. Host build scripts и proc macros сохраняют
предусмотренные Cargo настройки.

Cargo JSON сообщает только перечисленные поля профиля. Package overrides
для `codegen-units` и `strip`, внешние linkers, compiler wrappers, build scripts
и дополнительные Cargo config остаются доверенными входами окружения.
Перечень хешей в receipt покрывает указанные manifests и скрипты, а полная
привязка исходников к коммиту требует чистого checkout. Локальная запись
позволяет работать с незакоммиченными правками. Каноническое измерение требует
чистый Linux x64 checkout и совпадение двух сборок внутри одного CI job.
В каноническом шаге `LABCOLORS_WASM_FRESH=1` создаёт отдельные временные
каталоги результатов и промежуточных артефактов Cargo для каждого вызова.
Сборка проверяет, что Cargo сообщил об исполненной компиляции каждого
артефакта, и удаляет только принадлежащие ей временные каталоги.
Локальная сборка по умолчанию сохраняет обычный кэш.
Побайтовая повторимость между разными ОС требует отдельной проверки.

Контракт описан в [Cargo configuration](https://doc.rust-lang.org/cargo/reference/config.html),
[profile overrides](https://doc.rust-lang.org/cargo/reference/profiles.html#overrides),
[Cargo JSON](https://doc.rust-lang.org/cargo/reference/external-tools.html#artifact-messages)
и [исходниках wasm-pack 0.13.1](https://github.com/wasm-bindgen/wasm-pack/blob/v0.13.1/src/build/mod.rs).

Cargo обнаруживает проверки рядом с модулем Core и публичные контракты в
`crates/*/tests`. Для адресного запуска выбирайте крейт, тестовый файл,
затем функцию или группу. Например, публичный Program wire:

```sh
cargo test -p labcolors-core --test program_wire_contract --locked -- --list
cargo test -p labcolors-core --test program_wire_contract --locked -- check::
cargo test -p labcolors-core --test program_wire_contract --locked -- check::empty_input_is_rejected_as_wire_error --exact
```

`--list` только показывает обнаруженные проверки. Фильтр, включая `--exact`,
может вернуть код 0 без совпадений: последний пример подтверждает свойство,
только если итог показывает один выполненный тест (`1 passed`). В обычном выводе и при
ошибке виден путь, например `check::empty_input_is_rejected_as_wire_error`;
адресный успех не означает успех всего workspace. Этот же файл автоматически
входит в `cargo test --workspace --locked` в CI. Отдельные feature-, proof-
и платформенные проверки остаются в [CI workflow](../../.github/workflows/ci-worker.yml)
и не заменяются запуском одного контракта.

[Организация тестов Rust](https://doc.rust-lang.org/book/ch11-03-test-organization.html) и [штатная фильтрация Cargo](https://doc.rust-lang.org/cargo/commands/cargo-test.html) описывают эти границы для внешних участников.

## Зависимости

Для точных численных границ Core использует закреплённую `rustc_apfloat`;
её типы остаются внутри ядра. Сетевые и сериализационные библиотеки находятся
во внешних адаптерах. Состав зависимостей можно проверить командой ниже.
Инвентаризация исходников и метаданных выполняется отдельно:
[инструменты аудита](source-audit.md) не входят в API цветового ядра.

```sh
cargo tree -p labcolors-core --edges=no-dev
```

## Компилятор и обновления

`rust-toolchain.toml` выбирает один Rust для локальной работы, CI, WASM и native
сборок. `Cargo.toml` отдельно задаёт минимальную поддерживаемую версию Rust;
CI читает этот минимум из manifest и проверяет его с `--locked`. Обновление
компилятора не меняет обещанную совместимость библиотеки.

Dependabot еженедельно обновляет компилятор, Cargo, npm, Python и Actions.
Изменения minor/patch объединены; новая библиотечная версия выдерживает три дня
до обычного предложения, исправления уязвимостей идут отдельно без этой задержки.
`typescript-floor` сохраняет проверку минимально поддерживаемого TypeScript.

Версия CLI wasm-bindgen берётся из `Cargo.lock`. Установщик скачивает официальный
бинарник той же версии и до распаковки проверяет подписанную аттестацию
[неизменяемого GitHub release](https://cli.github.com/manual/gh_release_verify-asset).
Независимых version/checksum для этой пары больше нет; ошибка проверки прерывает
сборку без компиляции или скачивания запасного CLI.

Измеренный WASM budget задаёт верхний предел размера: уменьшение проходит без
переписывания baseline, рост останавливает проверку. Воспроизводимость отдельно
проверяется совпадением байтов двух сборок одной версии с одними входами.
