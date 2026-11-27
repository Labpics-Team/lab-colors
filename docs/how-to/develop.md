# Собрать и проверить Lab Colors

```sh
cargo test --workspace --locked
```

Версия Rust указана в [Cargo.toml](../../Cargo.toml). Команды сборки и проверки
npm-пакета находятся в [package.json](../../packages/colors/package.json).

```sh
npm --prefix packages/colors run build
```

Сборка требует Rust из `rust-toolchain.toml`, `wasm-pack` из
[`bench/wasm.json`](../../packages/colors/bench/wasm.json), Node.js и `tar` в PATH.
Она отключает только встроенный optimizer `wasm-pack`, затем выполняет
официальный Binaryen Node с теми же release, SHA-256 и флагами, что указаны
в `bench/wasm.json`. Оптимизированный WASM проверяется перед завершением.
Первый запуск скачивает архив с GitHub; следующие используют проверяемый
по SHA-256 пользовательский кэш. Для уже скачанного архива задайте
`LABCOLORS_BINARYEN_ARCHIVE`: его байты проходят ту же проверку.

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
