# Lab Colors

Lab Colors помогает задать цвета дизайн-системы один раз и адаптировать их к
поверхности, на которой они видны. В примере ниже у кнопки и ссылки один фирменный
синий. На светлом листе надпись на кнопке белая. Перенесите их на грифельный
фон — кнопка и ссылка посветлеют, а надпись станет тёмной.

![Одна кнопка и ссылка на двух фонах. Слева синий насыщеннее и надпись белая, справа синий светлее и надпись тёмная.](docs/media/accent-contexts.png)

Оба состояния отрисованы в браузере по примеру ниже. Между снимками приложение
изменило только фон.

[![npm](https://img.shields.io/npm/v/%40labpics%2Fcolors)](https://www.npmjs.com/package/@labpics/colors)
[![CI](https://github.com/Labpics-Team/lab-colors/actions/workflows/ci.yml/badge.svg)](https://github.com/Labpics-Team/lab-colors/actions/workflows/ci.yml)
[![Код: MIT](https://img.shields.io/badge/code-MIT-blue)](LICENSE)

## Как это работает

1. **Вы задаёте замысел.** Выберите опорные цвета и исходное окружение. Опишите,
   где используется каждая роль и какие свойства важны. В примере ссылка лежит
   на странице, а надпись — на заливке кнопки. Обеим нужна читаемость.
2. **Ядро Rust подбирает цвета в этом окружении.** Оно учитывает опору роли,
   фактическую подложку и прозрачность материала. У ссылки и кнопки могут
   получиться разные оттенки синего. Заливку и надпись ядро подбирает вместе.
3. **Приложение меняет вход.** При смене фона ядро пересчитывает связанные цвета.
   В браузере принятый фон и новые CSS-переменные вступают в силу вместе.
   Если подобрать допустимый результат не удалось, на экране остаётся предыдущий.

Имена и связи выбирает ваша команда. Например, вторичный текст на странице и
на карточке может брать цвет от одной опоры. Каждое применение учитывает свой
фон. Чтобы сохранить разницу в выразительности заголовка и подписи при смене
темы, их можно связать через `preserveTonalHierarchy`.

<details>
<summary>Посмотреть, как читается иерархия текста в трёх темах · 10 секунд</summary>

![Одна композиция последовательно становится светлой, тёмной и графитовой. Во всех трёх состояниях Primary заметнее Secondary.](docs/media/theme.webp)

Авторская анимация иллюстрирует желаемую иерархию. Палитры для неё подобраны вручную.
[Открыть исходное видео (MP4)](docs/media/theme.mp4)

</details>

## Попробовать в браузере

```sh
npm install @labpics/colors
```

Поместите кнопку и ссылку в общий контейнер.

```html
<div class="app">
  <button class="action">Опубликовать</button>
  <a href="/preview">Предпросмотр</a>
</div>
```

В JavaScript укажите один акцент и обе поверхности, на которых читается текст.

```js
import { colors } from "@labpics/colors";

const original = { canvas: "#f5f3ed" };
const c = colors({ reference: original });
const canvas = c.surface("canvas");
const accent = c.role("accent", "#0067f4");
const ink = c.role("ink", "#ffffff");
const action = accent.on(canvas);

const program = await c.compile({
  action,
  actionLabel: ink.on(action, { contrast: "wcag22-text" }),
  link: accent.on(canvas, { contrast: "wcag22-text" }),
});
const view = program.attach(document.querySelector(".app"), original);
```

```css
.app { background: var(--canvas); }
.action { background: var(--action); color: var(--actionLabel); }
.app a { color: var(--link); }
```

Измените фон, чтобы получить второе состояние на снимке.

```js
view.set({ canvas: "#44515f" });
```

Тот же граф можно подключить к нескольким контейнерам с разными фонами.
`fixed` закрепляет цвет, которому нужно оставаться неизменным, а `tone`
позволяет отдельной поверхности следовать собственной кривой светлоты.
[Настройки и жизненный цикл](packages/colors/README.md) ·
[Как переносится исходная композиция](docs/explanation/reference-appearance.md).

## Что известно о качестве цвета

Читаемость надписи проверяется на её итоговом фоне. Cleanliness отвечает на
другой вопрос — есть ли основание считать новую композицию чище прежней.
У каждой модели своя область применения. Например, точное восстановление
видимого растра при прежних условиях показа даёт основание для `NotDirtier`.
Общая оценка произвольного поля остаётся открытой задачей.
[Возможности, ограничения и исследования Cleanliness](docs/explanation/color-quality.md).

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

```sh
npm --prefix packages/colors run build
```

Сборка требует Rust из `rust-toolchain.toml`, `wasm-pack` из
[`bench/wasm.json`](packages/colors/bench/wasm.json), Node.js и `tar` в PATH.
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
и платформенные проверки остаются в [CI workflow](.github/workflows/ci-worker.yml)
и не заменяются запуском одного контракта.

[Организация тестов Rust](https://doc.rust-lang.org/book/ch11-03-test-organization.html) и [штатная фильтрация Cargo](https://doc.rust-lang.org/cargo/commands/cargo-test.html) описывают эти границы для внешних участников.

## Зависимости

Для точных численных границ Core использует закреплённую `rustc_apfloat`;
её типы остаются внутри ядра. Сетевые и сериализационные библиотеки находятся
во внешних адаптерах. Состав зависимостей можно проверить командой ниже.
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
