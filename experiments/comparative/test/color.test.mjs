// Golden-тесты арбитра: опубликованные референсные значения OKLab (Ottosson)
// и WCAG 2.1. Арбитр обязан быть верным до того, как судить системы.
import { describe, expect, it } from "vitest";
import {
  circularHueDist,
  hexToOklab,
  hexToOklch,
  hexToRgb,
  inGamut,
  maxChroma,
  median,
  oklchToHexGamutMapped,
  relativeLuminance,
  rgbToHex,
  wcagContrast,
} from "../src/color.mjs";

describe("OKLab (golden, Ottosson)", () => {
  const cases = [
    ["#FFFFFF", { L: 1.0, a: 0.0, b: 0.0 }],
    ["#FF0000", { L: 0.62796, a: 0.22486, b: 0.12585 }],
    ["#00FF00", { L: 0.86644, a: -0.23389, b: 0.1795 }],
    ["#0000FF", { L: 0.45201, a: -0.03246, b: -0.31153 }],
  ];
  for (const [hex, ref] of cases) {
    it(hex, () => {
      const got = hexToOklab(hex);
      expect(Math.abs(got.L - ref.L)).toBeLessThan(5e-4);
      expect(Math.abs(got.a - ref.a)).toBeLessThan(5e-4);
      expect(Math.abs(got.b - ref.b)).toBeLessThan(5e-4);
    });
  }
});

describe("WCAG 2.1 (golden)", () => {
  it("белый/чёрный = 21:1", () => {
    expect(wcagContrast("#FFFFFF", "#000000")).toBeCloseTo(21, 9);
  });
  it("красный на белом ≈ 3.998", () => {
    expect(wcagContrast("#FF0000", "#FFFFFF")).toBeCloseTo(3.9985, 3);
  });
  it("яркость белого = 1, чёрного = 0", () => {
    expect(relativeLuminance("#FFFFFF")).toBeCloseTo(1, 9);
    expect(relativeLuminance("#000000")).toBeCloseTo(0, 9);
  });
  it("контраст симметричен", () => {
    expect(wcagContrast("#336699", "#FFD000")).toBeCloseTo(wcagContrast("#FFD000", "#336699"), 12);
  });
});

describe("hex-парсинг и roundtrip", () => {
  it("hexToRgb/rgbToHex", () => {
    expect(rgbToHex(hexToRgb("#3a6ff2"))).toBe("#3A6FF2");
    expect(() => hexToRgb("#fff")).toThrow();
  });
  it("hex -> OKLCh -> hex (in-gamut, точность 8 бит)", () => {
    for (const hex of ["#007AFF", "#FF3B30", "#34C759", "#808080", "#FFD000"]) {
      const { L, C, h } = hexToOklch(hex);
      const back = hexToRgb(oklchToHexGamutMapped(L, C, h));
      const orig = hexToRgb(hex);
      for (const ch of ["r", "g", "b"]) {
        expect(Math.abs(back[ch] - orig[ch])).toBeLessThanOrEqual(1 / 255 + 1e-9);
      }
    }
  });
});

describe("sRGB-гамут", () => {
  it("maxChroma лежит на границе", () => {
    const cm = maxChroma(0.65, 30);
    expect(cm).toBeGreaterThan(0.1);
    expect(inGamut(0.65, cm - 1e-3, 30)).toBe(true);
    expect(inGamut(0.65, cm + 5e-3, 30)).toBe(false);
  });
  it("нейтраль всегда в гамуте", () => {
    expect(inGamut(0.5, 0, 0)).toBe(true);
    expect(maxChroma(0, 0)).toBe(0);
  });
});

describe("утилиты", () => {
  it("circularHueDist", () => {
    expect(circularHueDist(350, 10)).toBe(20);
    expect(circularHueDist(90, 90)).toBe(0);
    expect(circularHueDist(0, 180)).toBe(180);
  });
  it("median", () => {
    expect(median([3, 1, 2])).toBe(2);
    expect(median([1, 2, 3, 4])).toBe(2.5);
    expect(median([])).toBeNull();
  });
});
