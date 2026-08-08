import { brandFakeDocument, brandFakeElement } from "./fake-node-brand.mjs";

class FakeStyleDeclaration {
  #values = new Map();

  constructor(entries = []) {
    for (const [name, value] of entries) this.setProperty(name, value);
  }

  get length() {
    return this.#values.size;
  }

  item(index) {
    return [...this.#values.keys()][index] ?? "";
  }

  getPropertyValue(name) {
    return this.#values.get(name) ?? "";
  }

  setProperty(name, value) {
    if (!/^--[a-z0-9-]+$/u.test(name) || typeof value !== "string" || value.length === 0) {
      return;
    }
    this.#values.set(name, value.trim());
  }

  removeProperty(name) {
    const previous = this.getPropertyValue(name);
    this.#values.delete(name);
    return previous;
  }

  get cssText() {
    return [...this.#values].map(([name, value]) => `${name}: ${value};`).join(" ");
  }

  entries() {
    return [...this.#values];
  }
}

class FakeRule {
  constructor(selectorText, declarations = []) {
    this.selectorText = selectorText;
    this.style = new FakeStyleDeclaration(declarations);
  }

  get cssText() {
    const declarations = this.style.cssText;
    return declarations === ""
      ? `${this.selectorText} {}`
      : `${this.selectorText} { ${declarations} }`;
  }
}

function parseSheet(text) {
  if (text === "") return [];
  const match = /^(:root|:host) \{(?: (.*))?\}$/u.exec(text);
  if (!match) return [];
  const declarations = [];
  if (match[2]) {
    for (const part of match[2].split("; ")) {
      const declaration = part.endsWith(";") ? part.slice(0, -1) : part;
      if (declaration === "") continue;
      const separator = declaration.indexOf(": ");
      if (separator < 0) continue;
      declarations.push([declaration.slice(0, separator), declaration.slice(separator + 2)]);
    }
  }
  return [new FakeRule(match[1], declarations)];
}

/**
 * Structural DOM/CSSOM fake for the atomic output sink.
 *
 * `props` is a stable view of the element's effective custom properties. A
 * successful live `replaceSync` updates it once; scratch validation and failed
 * replacements never become observable.
 */
export function outputElement(initialInline = []) {
  const inlineStyle = new FakeStyleDeclaration(initialInline);
  const props = new Map(initialInline);
  const mutations = [];
  const control = {
    beforeLiveReplace: null,
    failNextLiveReplace: null,
  };
  let target;

  const root = brandFakeDocument({
    nodeType: 9,
    defaultView: null,
    adoptedStyleSheets: [],
  });

  function syncEffective(sheet) {
    props.clear();
    const rule = sheet.cssRules[0];
    if (rule) {
      for (const [name, value] of rule.style.entries()) props.set(name, value);
    }
    for (const [name, value] of inlineStyle.entries()) props.set(name, value);
  }

  class FakeCSSStyleSheet {
    constructor() {
      this.cssRules = [];
      this.replaceCalls = [];
    }

    replaceSync(text) {
      this.replaceCalls.push(text);
      const live = root.adoptedStyleSheets.includes(this);
      if (live) control.beforeLiveReplace?.(text);
      if (live && control.failNextLiveReplace !== null) {
        const error = control.failNextLiveReplace;
        control.failNextLiveReplace = null;
        throw error;
      }
      const next = parseSheet(text);
      this.cssRules = next;
      if (live) {
        syncEffective(this);
        mutations.push(["replace", new Map(props)]);
      }
    }
  }

  const realm = { CSSStyleSheet: FakeCSSStyleSheet };
  root.defaultView = realm;
  target = brandFakeElement({
    nodeType: 1,
    isConnected: true,
    ownerDocument: root,
    getRootNode: () => root,
    style: inlineStyle,
    props,
    mutations,
    outputHost: {
      root,
      realm,
      setBeforeLiveReplace(callback) {
        control.beforeLiveReplace = callback;
      },
      setBeforePublication(callback) {
        control.beforeLiveReplace = (text) => {
          const rule = parseSheet(text)[0];
          const values = new Map(rule?.style.entries() ?? []);
          callback(values, text);
        };
      },
      failNextLiveReplace(error = new Error("atomic host rejected replacement")) {
        control.failNextLiveReplace = error;
      },
      liveSheet() {
        return root.adoptedStyleSheets[0] ?? null;
      },
    },
  });
  root.documentElement = target;
  return target;
}
