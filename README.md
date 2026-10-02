# Lab Colors

Компилятор и runtime цветовых контрактов для приложений и дизайн-систем.
Вы объявляете источники цвета, связи, подложки и ограничения. Lab Colors
проверяет граф и вычисляет допустимые результаты при изменении наблюдаемых
условий. Приложение задаёт смысл токенов и применяет полученные цвета.

## Начать работу

Для JavaScript и TypeScript используйте пакет `@labpics/colors` с ядром на
Rust/WASM. [Установка и первый пример](packages/colors/README.md#установка)
показывают путь от объявления графа до чтения цвета: `compileProgramWire` →
`updateObserved` → outputs.

Для оценки из командной строки запустите пример из корня checkout с Rust
версии, указанной в [Cargo.toml](Cargo.toml), или новее:

```sh
cargo run --quiet --locked -p labcolors-evaluate-cli -- \
  crates/labcolors-evaluate-cli/examples/declared-point.json
```

Результат — JSON-отчёт с оценкой объявленной sRGB8-точки и LCEN-сертификатом.
[Руководство CLI](crates/labcolors-evaluate-cli/README.md) описывает вход,
ожидаемый результат и коды отказов.

## Границы библиотеки

- **Приложение** владеет именами токенов, состояниями компонентов, дизайном и
  отображением. Для ядра идентификаторы токенов непрозрачны.
- **[Core](crates/labcolors-core/README.md)** владеет цветовой математикой,
  графом зависимостей и проверкой ограничений. ProgramWire v1 задаёт бинарный
  формат графа; недопустимые входы возвращают явные ошибки.
- **WASM и CLI** передают запросы ядру. Отдельный
  [labcolors-transport](crates/labcolors-transport-cli/README.md) разбирает,
  сериализует и инспектирует LCEN-envelope без вычисления цвета.

Проверка относится к объявленной модели и переданным наблюдениям. Она не
подтверждает фактическую отрисовку браузера или человеческое восприятие;
DOM и CSS остаются у приложения.

## Разработка

Проверка Rust workspace из корня репозитория:

```sh
cargo test --workspace --locked
```

Требования к инструментам и команды JS/WASM-пакета указаны в
[packages/colors/package.json](packages/colors/package.json).
[GitHub Actions](https://github.com/Labpics-Team/lab-colors/actions/workflows/ci.yml)
показывает результаты CI.

У `labcolors-core` по умолчанию нет рантайм-зависимостей. Необязательная
возможность `ext09-extractor` подключает зависимости из
[манифеста Core](crates/labcolors-core/Cargo.toml).

## Лицензии

Исходный код — [MIT](LICENSE). Для встроенных данных действуют CC-BY-4.0 и
CC-BY-SA-4.0; источники и условия использования перечислены в
[NOTICE.md](crates/labcolors-core/NOTICE.md).
