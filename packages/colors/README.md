# Lab Colors

Библиотека адаптивных цветов для интерфейсов и дизайн-систем.
Объявите исходные цвета, места применения и связи между ними. Одна акцентная
роль получает подходящий цвет на каждой поверхности; заливка и её текст
подбираются вместе. При смене контекста приложение передаёт новые входы,
а Lab Colors пересчитывает связанный граф.

Cleanliness входит в замысел системы как самостоятельное качество конечной
композиции. [Контракт качества и карта исследований](https://github.com/Labpics-Team/lab-colors/blob/main/docs/explanation/color-quality.md)
разделяют реализованные возможности, ограниченные модели и обязательное
направление развития.

[![npm](https://img.shields.io/npm/v/%40labpics%2Fcolors)](https://www.npmjs.com/package/@labpics/colors)
[![CI](https://github.com/Labpics-Team/lab-colors/actions/workflows/ci.yml/badge.svg)](https://github.com/Labpics-Team/lab-colors/actions/workflows/ci.yml)
[![Код: MIT](https://img.shields.io/badge/code-MIT-blue)](https://github.com/Labpics-Team/lab-colors/blob/main/LICENSE)

## Установка

```sh
npm install @labpics/colors@^1.2
```

JavaScript и TypeScript, ESM. Ядро на Rust/WASM включено в пакет.
Сохраните lockfile, чтобы закрепить версию.

[Самостоятельный пример интерфейса](https://github.com/Labpics-Team/lab-colors/tree/main/examples/contextual-colors)
показывает связанные заливку и текст, локальный лист с фиксированным фоном и
полупрозрачный слой. В нём можно менять фон, исходные акценты и материал.

<a id="первый-маршрут"></a>

## Первый запуск в браузере

### Объявление ролей

```js
import { colors } from "@labpics/colors";

const c = colors();
const canvas = c.surface("canvas");
const accent = c.role("accent", "#0067f4");
const ink = c.role("ink", "#242424");

const fill = accent.on(canvas, { separation: [0.1, 2] });
const label = ink.on(fill, { contrast: "wcag22-text" });
const link = accent.on(canvas, { contrast: "wcag22-text" });
const text = ink.on(canvas, { contrast: "wcag22-text" });

const program = await c.compile({ fill, label, link, text });
```

`role` хранит неизменяемый исходный цвет и его идентичность. Каждый вызов
`.on(surface)` создаёт локальное применение: кнопка и ссылка используют одну
акцентную роль, но могут получить разные RGB. Повторная передача того же
handle `fill` означает одну общую заливку, в том числе при нескольких aliases.
Имена и смысл ролей принадлежат приложению.

`compile` проверяет граф и инициализирует WASM. Приоритет предпочтений задаётся
порядком создания локальных применений через `.on`: подложка объявлена раньше
зависимого текста. Порядок outputs и дополнительные aliases не меняют выбор.
Даже `compile({ label })` сохраняет участвующую в его композиции заливку и её
условия. Обязательные условия проверяются для всего результата. Исходные
массивы копируются при объявлении роли и не меняют её после создания.

### Подключение к интерфейсу

```html
<section id="project">
  <button>Открыть проект</button>
  <a href="#project-details">Все материалы</a>
  <p id="project-details">Документы и решения команды.</p>
</section>
```

```css
#project { background: var(--canvas); color: var(--text); padding: 24px; }
#project button { background: var(--fill); color: var(--label); }
#project a { color: var(--link); }
```

```js
const root = document.querySelector("#project");
const view = program.attach(root, { canvas: "#e6ddd0" });

// Например, при изменении поверхности рабочего пространства:
const update = view.set({ canvas: "#283548" });
if (update.state === "rejected") {
  console.log(update.proposed.state, update.proposed.diagnostics);
}
```

`attach` владеет CSS custom properties входов и outputs на подключённом
элементе. `set` получает полный новый контекст и атомарно заменяет одну
принадлежащую пакету таблицу CSS: фон и все цвета появляются вместе.
Если предложение не принято, прежняя поверхность и её готовые цвета остаются
на месте. `view.current` — принятый кадр, `view.lastUpdate` — исход последней
попытки.

Используйте эти переменные для фактических поверхностей соответствующих
элементов. Не переопределяйте их другими CSS-правилами и не меняйте принадлежащий
пакету stylesheet. До принятия кадра пакет проверяет применённые значения:
конфликт каскада или заблокированный stylesheet вызывает `color_host_changed`
и сохраняет прежний кадр. Если CSP требует nonce, передайте разрешённое
приложением значение: `program.attach(root, context, { nonce })`.
[Контракт CSS-подключения](https://github.com/Labpics-Team/lab-colors/blob/main/docs/reference/javascript-program.md#css-подключение)
описывает границы этой проверки.

`set` предназначен для контекста, которым владеет
приложение: если фон уже изменил другой renderer, прежние значения нельзя
считать проверенными для нового фона.

При удалении интерфейса вызовите `view.dispose()`, затем `program.dispose()`.
Программа может создавать независимые views; освобождение программы не
освобождает уже подключённые views.

### Загрузка WASM

В браузере без сборщика нужен import map, разрешающий `@labpics/colors` в
установленный ESM-пакет. Сервер или сборщик должен публиковать
`pkg/labcolors_bg.wasm` рядом с `pkg/labcolors.js`.
Если WASM перенесён, до `compile` задайте его URL:

```js
import init from "@labpics/colors";
await init({ module_or_path: wasmUrl });
```

В [готовом примере](https://github.com/Labpics-Team/lab-colors/tree/main/examples/contextual-colors)
есть import map и небольшой dev server; отдельный bundler не требуется.

## Node.js

В Node.js 22.11 или новее используйте ESM и передайте байты WASM до объявления
программы. Node.js не загружает `file:` URL через `fetch`.

```js
import { readFileSync } from "node:fs";
import init from "@labpics/colors";

await init({
  module_or_path: readFileSync(
    new URL(import.meta.resolve("@labpics/colors/pkg/labcolors_bg.wasm")),
  ),
});
```

Затем выполните [объявление ролей](#объявление-ролей) выше. Без DOM готовые
значения читаются через `resolve`:

### Результат без DOM

```js
for (const canvas of ["#e6ddd0", "#283548"]) {
  const frame = program.resolve({ canvas });
  console.log(frame);
}
program.dispose();
```

Пропущенный вход, `null` или `undefined` означает неизвестный контекст.
Лишнее имя или некорректный цвет — ошибка входа. Передавайте весь набор
поверхностей при каждом обновлении.

## Настройки

Обычная роль адаптируется по умолчанию. Core сохраняет её характер и пересекает
обязательные условия для конечной композиции. Параметры `identity`, `tone`,
`opacity`, `contrast`, `separation` и `fixed` описаны в
[справке авторского API](https://github.com/Labpics-Team/lab-colors/blob/main/docs/reference/javascript-program.md#настройки).

## Cleanliness в рабочем пути

Каждый готовый кадр содержит `frame.cleanliness`. Обычная декларация без
применимой модели возвращает `status: "not-assessed"`: успешный подбор по
идентичности и контрасту не выдаётся за оценку Cleanliness.

Если дизайн-система выбирает опубликованную номинальную конвенцию sRGB8,
она подключается одной настройкой и участвует в обычном автоматическом поиске:

```js
const c = colors({ cleanliness: "nominal-srgb8-v2" });
const canvas = c.surface("canvas");
const material = c.role("material", "#00c740", { maxChange: 0.01 }).on(canvas);
const program = await c.compile({ material });
const frame = program.resolve({ canvas: "#ffffff" });
if (frame.state === "ready") {
  console.log(frame.values.material, frame.cleanliness);
}
program.dispose();
```

Профиль проверяет опорный конечный encoded-sRGB8 цвет после наложения, включая
участвующие промежуточные поверхности. Core пересекает его с `identity`, `maxChange`,
контрастом и другими обязательствами, затем перепроверяет результат. Исходные
опоры и `fixed` сохраняются. Противоречие даёт обычную диагностику
`requirement: "cleanliness"`; `attach` сохраняет прежнюю композицию при отказе.

Оценка содержит `profile`, `basis: "declared-convention"`,
`scope: "declared-final-points"` и числа принятых, отвергнутых и недоступных
клеток. Это объём покрытия, не балл качества. Профиль не является общим
человеческим отношением `Cleaner`/`NotDirtier`, не оценивает пространственное
поле целиком и не включается скрыто: его границы могут исключать нужные
тёмные и приглушённые оттенки. Общая контекстная Cleanliness остаётся
обязательной незавершённой способностью продукта.

## Одно правило для разных композиций

Клиент может переиспользовать один закон поверхностей, сохраняя произвольную
топологию интерфейса. Следующее объявление независимо от первого примера:

```js
import { colors } from "@labpics/colors";

const c = colors();
const canvas = c.surface("canvas");
const paper = c.role("paper", "#efe6d8");
const warm = c.role("warm", "#b85d35");
const ink = c.role("ink", "#242424");
const text = { contrast: "wcag22-text" };
const layer = {
  tone: { from: canvas, curve: [[0, 0.22], [0.5, 0.58], [1, 0.97]] },
  separation: [0.02, 0.35],
};

// Первая композиция: общая поверхность → панель → текст.
const panel = paper.on(canvas, layer);
const panelText = ink.on(panel, text);

// Другая композиция: общая поверхность → прозрачный материал → текст.
const notice = warm.on(canvas, { ...layer, opacity: 0.72 });
const noticeText = ink.on(notice, text);

// Локальная поверхность не меняется при смене внешнего контекста.
const island = c.fixed("note-paper", "#f4eadb").on(canvas);
const islandText = ink.on(island, text);

const program = await c.compile({ panel, panelText, notice, noticeText, island, islandText });
```

Кривая `tone` задаётся парами `[светлота входа, предпочтительная светлота]`
в Oklab L. Нужны хотя бы две точки, возрастающие абсциссы и крайние значения
`0` и `1`; координаты лежат в `0..1`. Между точками Core интерполирует
линейно. Это предпочтение внутри допустимого множества. Значения кривой —
решение автора интерфейса; в Core нет названий тем или предписанной иерархии.

`opacity` сериализуется в alpha цвета, например `rgb(… / 0.72)`, и считается
как source-over над объявленной подложкой. Не заменяйте его CSS `opacity` на
всей группе: это другая композиция. Характер и предпочтение относятся к
опорному encoded-sRGB8 цвету; обязательные contrast и separation также
учитывают [численную точность наложения](https://github.com/Labpics-Team/lab-colors/blob/main/docs/explanation/material-emission.md).
Конкретный browser paint остаётся ответственностью renderer.
Обычная адаптация с `opacity: 1` меняет непрозрачный RGB.

### Изменение исходного замысла

Чтобы поменять authored цвет, материал или связи, скомпилируйте новую
декларацию и замените весь кадр:

```js
const replacement = view.replace(nextProgram, { canvas: "#283548" });
if (replacement.state === "committed") {
  // Новый handle имеет типы outputs новой программы; прежний больше не активен.
  const nextView = replacement.view;
  // Сохраните nextView у владельца интерфейса.
}
```

Успешная замена удаляет старые owned CSS variables и устанавливает новые в
той же транзакции. При отказе прежний view продолжает работать. Программы
не владеют runtime уже созданных views; каждый такой view освобождается
своим `dispose()`.

## Результат и диагностика

`resolve` возвращает готовый кадр с `values` либо причину, по которой свежего
результата нет. Допустимость результата и завершённость поиска лучшего значения
различаются. [Справка состояний и диагностики](https://github.com/Labpics-Team/lab-colors/blob/main/docs/reference/javascript-program.md#результат-и-диагностика)
описывает `ready`, `conflict`, `unresolved`, `unknown`, `stale` и типизированные ошибки.

## Низкоуровневый Program API

`colors()` компилирует тот же Program, что и `compileProgramWire` →
`ProgramRuntime` → `ProgramSnapshot`. V1 с фиксированными значениями,
V2 с явными конечными вариантами и V3 с вычисляемыми целями остаются доступны.
Ни DOM, ни авторские имена ролей не входят в математическое ядро.

[Справка JavaScript Program](https://github.com/Labpics-Team/lab-colors/blob/main/docs/reference/javascript-program.md)
описывает wire builders, output table, оценки, типизированные отказы, прямую
транзакцию runtime, point attachments, certificate envelopes и WASM ownership.
Точные публичные типы включены в пакет.
