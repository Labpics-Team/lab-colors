import { describe, expect, it } from "vitest";
import { relativeLuminance } from "../src/color.mjs";
import { BG_SWEEP, buildS1, buildS2, buildS3, buildS4, MCU_TONES } from "../src/systems.mjs";

const HEX6 = /^#[0-9A-F]{6}$/;

describe("BG_SWEEP", () => {
  it("11 нейтральных фонов, light -> dark", () => {
    expect(BG_SWEEP).toHaveLength(11);
    expect(BG_SWEEP[0].theme).toBe("light");
    expect(BG_SWEEP.at(-1).theme).toBe("dark");
    expect(BG_SWEEP.every((b) => HEX6.test(b.hex))).toBe(true);
  });
});

describe("S1 lab-colors (HEAD)", () => {
  const r = buildS1("#007AFF");
  it("лестница: 11 шагов, валидные hex, pos в [0,1]", () => {
    expect(r.ladder).toHaveLength(11);
    expect(r.ladder.every((s) => HEX6.test(s.hex))).toBe(true);
    expect(r.ladder[0].pos).toBe(0);
    expect(r.ladder.at(-1).pos).toBe(1);
  });
  it("native: 20 ролей с legalFloor на каждом из 11 фонов", () => {
    expect(r.native).toHaveLength(11);
    for (const { roles } of r.native) {
      expect(roles).toHaveLength(20);
      expect(roles.every((x) => HEX6.test(x.hex) && (x.floor === 4.5 || x.floor === 3))).toBe(true);
    }
  });
});

describe("S2 MCU", () => {
  const r = buildS2("#007AFF");
  it("11 тонов 100 -> 0, белый и чёрный на краях", () => {
    expect(r.ladder).toHaveLength(MCU_TONES.length);
    expect(r.ladder[0]).toMatchObject({ tone: 100, hex: "#FFFFFF" });
    expect(r.ladder.at(-1)).toMatchObject({ tone: 0, hex: "#000000" });
  });
  it("light -> dark по яркости на краях", () => {
    expect(relativeLuminance(r.ladder[0].hex)).toBeGreaterThan(
      relativeLuminance(r.ladder.at(-1).hex),
    );
  });
});

describe("S3 наивные OKLCH-рампы", () => {
  it("clip и gamut-map дают 11 валидных шагов", () => {
    for (const mode of ["clip", "gamut"]) {
      const r = buildS3("#FF3B30", mode);
      expect(r.ladder).toHaveLength(11);
      expect(r.ladder.every((s) => HEX6.test(s.hex))).toBe(true);
    }
  });
  it("стратегии различаются там, где сид-хрома выходит за гамут", () => {
    const a = buildS3("#FF3B30", "clip").ladder.map((s) => s.hex);
    const b = buildS3("#FF3B30", "gamut").ladder.map((s) => s.hex);
    expect(a).not.toEqual(b);
  });
});

describe("S4 Radix Colors", () => {
  it("хроматический сид -> хроматическая шкала, 12 шагов", () => {
    const r = buildS4("#FF0000");
    expect(r.ladder).toHaveLength(12);
    expect(r.ladder[0].step).toBe(1);
    expect(r.ladder.at(-1).step).toBe(12);
    expect(r.scale).not.toBe("gray");
    expect(typeof r.step9Hue).toBe("number");
  });
  it("ахроматический сид -> gray", () => {
    expect(buildS4("#808080").scale).toBe("gray");
  });
});
