# Lab Colors

Библиотека связанного управления цветом для интерфейсов и дизайн-систем.
Вы задаёте характер ролей, их применения и правила поверхностей. Lab Colors
вычисляет цвета в текущем контексте и согласованно обновляет весь результат.

Cleanliness — одна из основных целей: полезно улучшать конечную композицию,
сохраняя замысел, цветовую идентичность, различимость и материал. Направление,
реализованные возможности и остающиеся задачи разделены в
[контракте качества](docs/explanation/color-quality.md). Исследовательская база
и история планов доступны оттуда; контраст покрывает отдельное условие.

[![npm](https://img.shields.io/npm/v/%40labpics%2Fcolors)](https://www.npmjs.com/package/@labpics/colors)
[![CI](https://github.com/Labpics-Team/lab-colors/actions/workflows/ci.yml/badge.svg)](https://github.com/Labpics-Team/lab-colors/actions/workflows/ci.yml)
[![Код: MIT](https://img.shields.io/badge/code-MIT-blue)](LICENSE)

## Установка

```sh
npm install @labpics/colors@^1.2
```

JavaScript и TypeScript, ESM. Ядро на Rust/WASM включено в пакет.
[Первый запуск в браузере и Node.js](packages/colors/README.md#первый-запуск-в-браузере).

## Одна роль — разные применения

Синий акцент задаётся один раз. Заливка, текстовая ссылка и надпись на заливке
связаны через реальные поверхности:

```js
import { colors } from "@labpics/colors";

const c = colors();
const canvas = c.surface("canvas");
const accent = c.role("accent", "#0067F4");
const ink = c.role("ink", "#ffffff");
const action = accent.on(canvas);
const program = await c.compile({
  action,
  actionLabel: ink.on(action, { contrast: "wcag22-text" }),
  link: accent.on(canvas, { contrast: "wcag22-text" }),
});

const view = program.attach(document.querySelector(".app"), { canvas: "#f5f3ed" });
view.set({ canvas: "#151719" });
```

```css
.app { background: var(--canvas); }
.action { background: var(--action); color: var(--actionLabel); }
a { color: var(--link); }
```

Каждый `role.on(surface)` создаёт местное применение общего характера.
Заливка одновременно служит поверхностью для надписи. Повторное использование
того же применения сохраняет один общий цвет. Значения вычисляются в Core;
перечислять кандидаты или HEX для каждой темы не требуется.

[Запустить локальный пример](examples/contextual-colors/README.md): карточка, связанный
акцент, независимый лист и прозрачный материал. Меняйте фон и исходные цвета,
сохраняя одну декларацию связей.

## Правила принадлежат вашей дизайн-системе

Светлота поверхности может следовать собственной кривой от контекста. У разных
поверхностей могут быть разные кривые: чередование светлых слоёв и возрастающая
светлота тёмных задаются той же формой данных. Имена уровней и устройство
компонентов выбирает приложение.

Цветовой характер задаётся источником и настраиваемой геометрией оттенка,
насыщенности и светлоты. `separation` регулирует отделение поверхности от фона,
`contrast` добавляет требование к тексту или графике. `opacity` описывает материал:
подбор и проверка учитывают итог наложения. Обычная адаптация работает и при
полной непрозрачности. Для неизменного фирменного цвета или локального листа
есть явный `fixed`.

`view.set` готовит всю систему и одним действием принимает фон со связанными
цветами. При несовместимом предложении остаётся предыдущее согласованное
состояние. Результат объясняет конкретные нарушенные условия; незавершённый
поиск лучшего варианта сохраняет уже найденное пригодное решение.

[Настройки и жизненный цикл](packages/colors/README.md) ·
[Модель цветовой системы](docs/explanation/color-system.md) ·
[Характер цвета и исследования](docs/explanation/color-quality.md).

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
