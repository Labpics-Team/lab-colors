# Подбирать цвет при смене фона

Пример выбирает один цвет текста, подходящий для всех переданных фонов.
Предпочтение чёрного или белого задаёт приложение; ограничение WCAG 2.2
проверяет итоговую композицию. Требуется Node.js 22.11 или новее.

В каталоге приложения установите пакет:

```sh
npm install @labpics/colors@^1
```

Сохраните в `adaptive-color.mjs`:

```js
import { readFile } from "node:fs/promises";
import init, { compileProgramWire } from "@labpics/colors";
import {
  ProgramWireBuilderV2,
  SURROUND_AVERAGE_V1,
  WCAG22_SC143_TEXT_DEFAULT_V1,
} from "@labpics/colors/program-wire/abi-v2.js";

await init({
  module_or_path: await readFile(
    new URL(import.meta.resolve("@labpics/colors/pkg/labcolors_bg.wasm")),
  ),
});

const program = new ProgramWireBuilderV2()
  .finiteTarget(21, [
    { id: 201, rgb: [0, 0, 0], opacity: 1 },
    { id: 202, rgb: [255, 255, 255], opacity: 1 },
  ])
  .selectionRelease(1n, [
    [{ id: 1, choices: [{ target: 21, candidate: 201 }] }],
    [{ id: 2, choices: [{ target: 21, candidate: 202 }] }],
  ])
  .surfaceInputPort(31)
  .solidPaint(41, 21)
  .inputSurface(51, 31)
  .sourceOverOccurrence(61, 41, 51, 64, 0.2, SURROUND_AVERAGE_V1)
  .presentationRoot(71, 61)
  .presentationTarget(71, 61)
  .wcag22VisibleUnary(true, 81, 61, WCAG22_SC143_TEXT_DEFAULT_V1)
  .output(91, 41)
  .finish();

const runtime = compileProgramWire(program, 1);
const cases = [
  [[255, 255, 255]],
  [[0, 0, 0]],
  [[255, 255, 255], [0, 0, 0]],
];
try {
  for (const [index, backgrounds] of cases.entries()) {
    const snapshot = runtime.updateObserved(
      BigInt(index + 1),
      new Uint32Array(backgrounds.map((_, id) => id + 1)),
      new Uint8Array(backgrounds.flat()),
      1,
    );
    try {
      console.log(JSON.stringify({
        state: snapshot.state,
        rgb: snapshot.state === "ready" ? [...snapshot.outputRgb(0)] : null,
      }));
    } finally {
      snapshot.free();
    }
  }
} finally {
  runtime.free();
}
```

Запустите:

```sh
node adaptive-color.mjs
```

На белом фоне получается чёрный текст, на чёрном получается белый. В третьем случае
ни один из двух вариантов не подходит одновременно для обоих фонов:
результат имеет `state: "failed"` и `rgb: null`.

В приложении передавайте фактически известные фоны и увеличивайте ревизию
при каждом обновлении. При `failed` или `stale` скрывайте ранее установленный
результат либо используйте отдельно проверенную композицию. Состояние `ready`
позволяет применить полученные RGB и opacity; DOM, CSS и получение наблюдений
принадлежат приложению. ID в примере локальны для графа и не задают смысл токенов.
