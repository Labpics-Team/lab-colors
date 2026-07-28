const INVALID_RGB24 = 0xFFFFFFFF;

/** Lower the real resolver projection into the physical occurrence shape that
 * the boundary benchmark measures. Unsupported diagnostic roles are not
 * emitted occurrences and therefore do not enter the measured row. */
export function benchmarkOccurrencesFromRoles(roles, packRgb24) {
  return Object.values(roles).flatMap((role) => {
    if (role.kind === "color") {
      return [{ sourceRgb24: packRgb24(role.hex), opacity: 1 }];
    }
    if (role.kind === "translucent") {
      return [{ sourceRgb24: packRgb24(role.tintHex), opacity: role.alpha }];
    }
    return [];
  });
}

/** Materialize each occurrence on one current backdrop into caller-owned
 * scratch. `composite` stays injected so the benchmark helper remains a pure
 * behavioral oracle over the same Core operation used by the controller. */
export function materializeOccurrences(occurrences, backdrop, row, composite) {
  if (row.length !== occurrences.length) {
    throw new RangeError("benchmark occurrence row must match the occurrence set");
  }

  for (let i = 0; i < occurrences.length; i++) {
    const occurrence = occurrences[i];
    if (occurrence.opacity === 1) {
      row[i] = occurrence.sourceRgb24;
      continue;
    }
    const visible = composite(
      occurrence.sourceRgb24,
      occurrence.opacity,
      backdrop,
    );
    if (visible === INVALID_RGB24) {
      throw new RangeError(
        `benchmark: Core rejected admitted opacity ${occurrence.opacity}`,
      );
    }
    row[i] = visible;
  }
  return row;
}
