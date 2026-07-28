const CHANNEL_MAX = 0xff;
const RGB24_MAX = 0xffffff;

export function rustCacheCapacity(source) {
  const match = source.match(
    /^\s*(?:pub(?:\([^)]*\))?\s+)?const\s+CACHE_CAPACITY\s*:\s*usize\s*=\s*([\d_]+)(?:usize)?\s*;/mu,
  );
  const literal = match?.[1].replaceAll("_", "");
  const capacity = Number(literal);
  if (!Number.isSafeInteger(capacity) || capacity < 1 || capacity >= RGB24_MAX) {
    throw new Error("Rust cache capacity is absent or outside the finite RGB24 domain");
  }
  return capacity;
}

export function buildMissRing(centerRgb24, capacity) {
  if (!Number.isInteger(centerRgb24) || centerRgb24 < 0 || centerRgb24 > RGB24_MAX) {
    throw new TypeError("centerRgb24 must be one encoded-sRGB8 word");
  }
  if (!Number.isSafeInteger(capacity) || capacity < 1 || capacity >= RGB24_MAX) {
    throw new RangeError("capacity must leave one non-centre RGB24 cache-miss key");
  }

  const center = [
    centerRgb24 >>> 16,
    (centerRgb24 >>> 8) & CHANNEL_MAX,
    centerRgb24 & CHANNEL_MAX,
  ];
  const required = capacity + 1;
  const backgrounds = [];
  // Chebyshev shells enumerate each neighbouring RGB point once and keep the
  // successful benchmark fixture as local to the solve background as possible.
  for (let radius = 1; radius <= CHANNEL_MAX && backgrounds.length < required; radius++) {
    for (let dr = -radius; dr <= radius && backgrounds.length < required; dr++) {
      for (let dg = -radius; dg <= radius && backgrounds.length < required; dg++) {
        for (let db = -radius; db <= radius && backgrounds.length < required; db++) {
          if (Math.max(Math.abs(dr), Math.abs(dg), Math.abs(db)) !== radius) continue;
          const [r, g, b] = [center[0] + dr, center[1] + dg, center[2] + db];
          if (
            r < 0 || r > CHANNEL_MAX ||
            g < 0 || g > CHANNEL_MAX ||
            b < 0 || b > CHANNEL_MAX
          ) {
            continue;
          }
          const word = (r << 16) | (g << 8) | b;
          backgrounds.push(`#${word.toString(16).padStart(6, "0").toUpperCase()}`);
        }
      }
    }
  }
  if (backgrounds.length !== required) {
    throw new Error("RGB24 domain cannot provide a cache-capacity-plus-one ring");
  }
  return backgrounds;
}
