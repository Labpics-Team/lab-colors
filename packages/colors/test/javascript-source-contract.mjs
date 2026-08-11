function tokens(source) {
  const result = [];
  for (let index = 0; index < source.length; ) {
    const character = source[index];
    const next = source[index + 1];
    if (/\s/u.test(character)) {
      index += 1;
    } else if (character === "/" && next === "/") {
      index = source.indexOf("\n", index + 2);
      if (index === -1) break;
    } else if (character === "/" && next === "*") {
      index = source.indexOf("*/", index + 2);
      if (index === -1) throw new Error("unterminated JavaScript block comment");
      index += 2;
    } else if (character === '"' || character === "'") {
      const quote = character;
      const start = index;
      index += 1;
      while (index < source.length && source[index] !== quote) {
        index += source[index] === "\\" ? 2 : 1;
      }
      if (index >= source.length) throw new Error("unterminated JavaScript string literal");
      const raw = source.slice(start, index + 1);
      result.push({ type: "string", value: quote === '"' ? JSON.parse(raw) : raw.slice(1, -1) });
      index += 1;
    } else if (character === "`") {
      index += 1;
      while (index < source.length && source[index] !== "`") {
        index += source[index] === "\\" ? 2 : 1;
      }
      if (index >= source.length) throw new Error("unterminated JavaScript template literal");
      index += 1;
    } else if (/[A-Za-z_$]/u.test(character)) {
      const start = index;
      index += 1;
      while (/[A-Za-z0-9_$]/u.test(source[index] ?? "")) index += 1;
      result.push({ type: "identifier", value: source.slice(start, index) });
    } else {
      result.push({ type: "punctuator", value: character });
      index += 1;
    }
  }
  return result;
}

export function chromeArguments(source) {
  const sourceTokens = tokens(source);
  for (let index = 0; index < sourceTokens.length - 4; index += 1) {
    if (
      sourceTokens[index].type !== "string" ||
      sourceTokens[index].value !== "goog:chromeOptions" ||
      sourceTokens[index + 1].value !== ":" ||
      sourceTokens[index + 2].value !== "{"
    ) {
      continue;
    }
    let objectDepth = 1;
    for (let cursor = index + 3; cursor < sourceTokens.length && objectDepth > 0; cursor += 1) {
      const token = sourceTokens[cursor];
      if (token.value === "{") objectDepth += 1;
      if (token.value === "}") objectDepth -= 1;
      if (
        objectDepth === 1 &&
        token.type === "identifier" &&
        token.value === "args" &&
        sourceTokens[cursor + 1]?.value === ":" &&
        sourceTokens[cursor + 2]?.value === "["
      ) {
        const args = [];
        let nestedDepth = 0;
        let element = [];
        for (let argument = cursor + 3; argument < sourceTokens.length; argument += 1) {
          const argumentToken = sourceTokens[argument];
          if (nestedDepth === 0 && (argumentToken.value === "," || argumentToken.value === "]")) {
            if (element.length === 1 && element[0].type === "string") args.push(element[0].value);
            element = [];
            if (argumentToken.value === "]") return args;
            continue;
          }
          if (["[", "{", "("].includes(argumentToken.value)) nestedDepth += 1;
          if (["]", "}", ")"].includes(argumentToken.value)) nestedDepth -= 1;
          element.push(argumentToken);
        }
        throw new Error("unterminated goog:chromeOptions args array");
      }
    }
  }
  throw new Error("goog:chromeOptions args array is absent");
}
