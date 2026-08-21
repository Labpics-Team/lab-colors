# @labpics/colors

Компилятор и runtime проверяемых цветовых Program-графов.

Клиент передаёт канонические Program wire-байты. Ядро атомарно проверяет граф, создаёт Session, принимает наблюдения среды и возвращает только сертифицированные Paint outputs. Recipe-меню ролей и специальные Material/Glow runtime-пути удалены из публичного API.

## Установка

```sh
npm install @labpics/colors
```

## Первый маршрут

```ts
import init, { compileProgramWire } from "@labpics/colors";
import { ProgramWireBuilderV1 } from "@labpics/colors/program-wire/abi-v1.js";

await init();
// Canonical LCPW v1 bytes are produced by ProgramWireBuilderV1 or any
// implementation-independent encoder of the same contract. The builder
// lives on a sub-path: it is an authoring aid, not part of the runtime
// facade that ships with every consumer bundle.
const programBytes = new ProgramWireBuilderV1()
  .addSource(/* ... */)
  .build();
const runtime = compileProgramWire(programBytes, 1);
const snapshot = runtime.updateObserved(
  1n,
  new Uint32Array([1]),
  new Uint8Array([255, 255, 255]),
  1,
);

if (snapshot.state === "ready") {
  for (let index = 0; index < snapshot.outputCount(); index += 1) {
    console.log(
      snapshot.outputSlot(index),
      snapshot.outputRgb(index),
      snapshot.outputOpacity(index),
    );
  }
}
```

Канонические `LCPW` v1 bytes создаются `ProgramWireBuilderV1` или другим реализационно-независимым энкодером того же контракта.

## Контракт

- Один публичный runtime-root: `compileProgramWire` → `ProgramRuntime` → `ProgramSnapshot`.
- Обновление атомарно: отказ не публикует частичный state.
- Output появляется только вместе с сертификатом полного hard-support.
- Канонические байты имеют одну `ContentIdentity`.
- Невалидные wire-байты, графы, observation и resource bounds возвращают typed-отказы; fallback отсутствует.
- DOM и CSS не входят в ядро. Применение output принадлежит приложению.

## Дополнительные функции

- `evaluateWcag22` — точная оценка WCAG 2.2 в finite sRGB8/Q55-профиле.
- `numericalCapabilityManifest` — манифест численных возможностей и доказательств сборки.

Публикация registry/deploy не является частью этого изменения: пакет проверяется из точного CI tarball до отдельного release-гейта.
