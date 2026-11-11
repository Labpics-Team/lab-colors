# Lab Colors

Библиотека адаптивных цветов для интерфейсов и дизайн-систем.
Вы задаёте цвета, фоны и ограничения, например контраст текста.
Lab Colors выбирает подходящие значения с учётом наложения слоёв
и пересчитывает результат, когда приложение передаёт изменения фона.

[![npm](https://img.shields.io/npm/v/%40labpics%2Fcolors)](https://www.npmjs.com/package/@labpics/colors)
[![CI](https://github.com/Labpics-Team/lab-colors/actions/workflows/ci.yml/badge.svg)](https://github.com/Labpics-Team/lab-colors/actions/workflows/ci.yml)
[![Код: MIT](https://img.shields.io/badge/code-MIT-blue)](LICENSE)

## Установка

```sh
npm install @labpics/colors@^1
```

JavaScript и TypeScript, ESM. Ядро на Rust/WASM включено в пакет.
[Настройка для браузера и Node.js](packages/colors/README.md#первый-запуск).

## Цвет под фон

[Готовый пример](docs/how-to/adapt-contrast.md) выбирает чёрный или белый текст
по требованию контраста WCAG 2.2. На белом фоне получается чёрный текст,
на чёрном получается белый. Приложение задаёт варианты цвета и их приоритет.

Если один цвет должен подходить сразу для нескольких фонов, библиотека
проверяет каждый из них. Когда подходящего варианта нет, она возвращает
`failed`. Пример показывает оба исхода и обработку результата.

## Как это устроено

Цвета, их связи, слои и ограничения образуют граф. Компилятор проверяет его,
а runtime принимает изменения фона и вычисляет результат. Имена токенов,
состояния компонентов и применение цветов в DOM или CSS задаёт приложение.

Проверки относятся к переданным цветам и фонам. Отрисовку и читаемость
готового интерфейса проверяйте в самом приложении.

## Документация

[JavaScript и WASM](packages/colors/README.md) ·
[Ядро на Rust](crates/labcolors-core/README.md) ·
[Swift для macOS и iOS](bindings/swift/DISTRIBUTION.md) ·
[Kotlin для Android](bindings/kotlin/README.md) ·
[Оценка через CLI](crates/labcolors-evaluate-cli/README.md) ·
[Все руководства](docs/README.md).

## Разработка

```sh
cargo test --workspace --locked
```

Версия Rust указана в [Cargo.toml](Cargo.toml). Команды сборки и проверки
npm-пакета находятся в [package.json](packages/colors/package.json).

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
и платформенные проверки остаются в [CI workflow](.github/workflows/ci-worker.yml)
и не заменяются запуском одного контракта.

[Организация тестов Rust](https://doc.rust-lang.org/book/ch11-03-test-organization.html) и [штатная фильтрация Cargo](https://doc.rust-lang.org/cargo/commands/cargo-test.html) описывают эти границы для внешних участников.

## Зависимости

У `labcolors-core` ноль рантайм-зависимостей во всех наборах возможностей.
Инвентаризация исходников и метаданных выполняется отдельно:
[инструменты аудита](docs/how-to/source-audit.md) не входят в API цветового ядра.

```sh
cargo tree -p labcolors-core --edges=no-dev
```

## Лицензии

Код: [MIT](LICENSE). Встроенные данные: CC-BY-4.0 и CC-BY-SA-4.0.
Источники и условия использования перечислены в
[NOTICE.md](crates/labcolors-core/NOTICE.md).
