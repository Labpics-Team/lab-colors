import init, { compileProgramWire } from "@labpics/colors";

// 1. Инициализация WASM-модуля
await init();

// 2. Компиляция канонических Program wire байтов
const wireBytes = new Uint8Array([/* ... канонические LCPW v1 байты ... */]);
const runtime = compileProgramWire(wireBytes, 1);

// 3. Передача наблюдаемого сценария и получение сертифицированного снимка
const snapshot = runtime.updateObserved(
  1n,                              // ревизия (bigint)
  new Uint32Array([1]),            // ID сценариев
  new Uint8Array([255, 255, 255]), // значения поверхностей (row-major RGB)
  1                                // количество поверхностей на сценарий
);

// 4. Чтение сертифицированных выходов
if (snapshot.state === "ready" && snapshot.outputCount() > 0) {
  const slot = snapshot.outputSlot(0);
  const rgb = snapshot.outputRgb(0);      // Uint8Array [R, G, B]
  const opacity = snapshot.outputOpacity(0); // number 0..1
}

// 5. Явное управление жизненным циклом
runtime.free(); // или Symbol.dispose через `using`