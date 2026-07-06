// Предрегистрация требует: повторный прогон байт-идентичен (RNG нигде нет).
import { describe, expect, it } from "vitest";
import { computeResults } from "../src/run.mjs";

describe("детерминизм прогона", () => {
  it("два вычисления дают байт-идентичный JSON", () => {
    const a = JSON.stringify(computeResults());
    const b = JSON.stringify(computeResults());
    expect(a).toBe(b);
  }, 120_000);

  it("ошибок резолва S1 нет (иначе — честно в отчёт)", () => {
    const r = computeResults();
    const errs = r.perSeed.filter((e) => e.systems.s1.error);
    expect(errs.map((e) => `${e.id}: ${e.systems.s1.error}`)).toEqual([]);
  }, 120_000);
});
