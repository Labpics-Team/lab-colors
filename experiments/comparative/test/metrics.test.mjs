import { describe, expect, it } from "vitest";
import { oklchToHexGamutMapped } from "../src/color.mjs";
import { chromaUtilization, hueDrift, m1aBlind, monotonicity } from "../src/metrics.mjs";

const grayLadder = (n, from = 0.97, to = 0.12) =>
  Array.from({ length: n }, (_, i) => ({
    hex: oklchToHexGamutMapped(from + (i * (to - from)) / (n - 1), 0, 0),
    pos: i / (n - 1),
  }));

describe("M1-A (слепой протокол)", () => {
  it("11 шагов -> 21 пара с расстоянием >= 0.5", () => {
    const r = m1aBlind(grayLadder(11));
    expect(r.pairs).toBe(21);
    expect(r.ge45).toBeLessThanOrEqual(r.pairs);
    expect(r.ge30).toBeGreaterThanOrEqual(r.ge45);
  });
  it("крайняя пара почти белый/почти чёрный проходит 4.5", () => {
    const r = m1aBlind([
      { hex: "#FFFFFF", pos: 0 },
      { hex: "#000000", pos: 1 },
    ]);
    expect(r).toEqual({ pairs: 1, ge45: 1, ge30: 1 });
  });
});

describe("M2 (дрейф hue)", () => {
  it("ахроматичная лестница -> null", () => {
    expect(hueDrift(grayLadder(5), 120)).toBeNull();
  });
  it("постоянный hue -> дрейф ~0", () => {
    const ladder = [0.7, 0.5, 0.35].map((L, i) => ({
      hex: oklchToHexGamutMapped(L, 0.12, 240),
      pos: i / 2,
    }));
    const r = hueDrift(ladder, 240);
    expect(r.counted).toBe(3);
    expect(r.mean).toBeLessThan(1.5);
    expect(r.max).toBeLessThan(3);
  });
});

describe("M3 (утилизация хромы)", () => {
  it("шаги вне L-окна не считаются", () => {
    const r = chromaUtilization([{ hex: oklchToHexGamutMapped(0.95, 0.02, 100), pos: 0 }]);
    expect(r).toBeNull();
  });
  it("хрома на границе гамута -> утилизация ~1", () => {
    const r = chromaUtilization([{ hex: oklchToHexGamutMapped(0.55, 0.5, 30), pos: 0 }]);
    expect(r.counted).toBe(1);
    expect(r.median).toBeGreaterThan(0.9);
    expect(r.median).toBeLessThanOrEqual(1);
  });
});

describe("M4 (монотонность)", () => {
  it("строго убывающая лестница -> 0 нарушений", () => {
    expect(monotonicity(grayLadder(11)).violations).toBe(0);
  });
  it("перевёрнутая лестница -> все переходы нарушены", () => {
    expect(monotonicity(grayLadder(11, 0.12, 0.97)).violations).toBe(10);
  });
  it("повтор шага -> нарушение", () => {
    expect(
      monotonicity([
        { hex: "#888888", pos: 0 },
        { hex: "#888888", pos: 1 },
      ]).violations,
    ).toBe(1);
  });
});
