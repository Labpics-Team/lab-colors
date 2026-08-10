const elements = new WeakSet();
const documents = new WeakSet();
const shadowRoots = new WeakSet();
const ambientState = new WeakMap();

function propertyDescriptor(receiver, property) {
  let owner = receiver;
  while (owner !== null && !PLATFORM_PROTOTYPES.has(owner)) {
    const descriptor = Object.getOwnPropertyDescriptor(owner, property);
    if (descriptor !== undefined) return descriptor;
    owner = Object.getPrototypeOf(owner);
  }
  return undefined;
}

function ownValue(receiver, property) {
  const descriptor = propertyDescriptor(receiver, property);
  if (descriptor === undefined) throw new TypeError(`missing test ${property}`);
  if ("value" in descriptor) return descriptor.value;
  if (typeof descriptor.get !== "function") throw new TypeError(`unreadable test ${property}`);
  return Reflect.apply(descriptor.get, receiver, []);
}

function writeOwn(receiver, property, value) {
  const descriptor = propertyDescriptor(receiver, property);
  if (descriptor === undefined) throw new TypeError(`missing test ${property}`);
  if ("value" in descriptor) {
    if (descriptor.writable !== true) throw new TypeError(`readonly test ${property}`);
    receiver[property] = value;
    return;
  }
  if (typeof descriptor.set !== "function") throw new TypeError(`readonly test ${property}`);
  Reflect.apply(descriptor.set, receiver, [value]);
}

class PlatformNode {
  get nodeType() {
    if (documents.has(this)) return 9;
    if (shadowRoots.has(this)) return 11;
    if (elements.has(this)) return 1;
    throw new TypeError("receiver does not implement the test Node interface");
  }

  get ownerDocument() {
    const state = ambientState.get(this);
    if (state !== undefined) return state.document ?? null;
    if (documents.has(this)) return null;
    if (elements.has(this) || shadowRoots.has(this)) return ownValue(this, "ownerDocument");
    throw new TypeError("receiver does not implement the test Node interface");
  }

  get isConnected() {
    const state = ambientState.get(this);
    if (state !== undefined) return state.connected === true;
    if (elements.has(this)) return ownValue(this, "isConnected");
    return false;
  }

  getRootNode() {
    const state = ambientState.get(this);
    if (state !== undefined) return state.root ?? this;
    if (elements.has(this)) {
      const callable = ownValue(this, "getRootNode");
      if (typeof callable !== "function") throw new TypeError("invalid test getRootNode");
      return Reflect.apply(callable, this, []);
    }
    if (documents.has(this) || shadowRoots.has(this)) return this;
    throw new TypeError("receiver does not implement the test Node interface");
  }
}

class PlatformStyleDeclaration {
  get length() {
    const state = ambientState.get(this);
    if (state !== undefined) return state.names.length;
    return ownValue(this, "length");
  }

  item(index) {
    const state = ambientState.get(this);
    if (state !== undefined) return state.names[index] ?? "";
    const callable = ownValue(this, "item");
    if (typeof callable !== "function") throw new TypeError("invalid test style.item");
    return Reflect.apply(callable, this, [index]);
  }
}

class PlatformElement extends PlatformNode {
  get shadowRoot() {
    const state = ambientState.get(this);
    if (state !== undefined) return state.shadowRoot ?? null;
    if (elements.has(this)) {
      const descriptor = Object.getOwnPropertyDescriptor(this, "shadowRoot");
      return descriptor === undefined ? null : ownValue(this, "shadowRoot");
    }
    throw new TypeError("receiver does not implement the test Element interface");
  }

  get style() {
    const state = ambientState.get(this);
    if (state !== undefined) return state.style;
    if (elements.has(this)) return ownValue(this, "style");
    throw new TypeError("receiver does not implement the test Element interface");
  }

  attachShadow(init) {
    const state = ambientState.get(this);
    if (state === undefined || init?.mode !== "open" || state.shadowRoot) {
      throw new TypeError("invalid test attachShadow receiver or mode");
    }
    const root = new PlatformShadowRoot();
    shadowRoots.add(root);
    ambientState.set(root, {
      adopted: [],
      document: state.document,
      host: this,
      mode: "open",
      root,
    });
    state.shadowRoot = root;
    return root;
  }
}

class PlatformDocument extends PlatformNode {
  get documentElement() {
    const state = ambientState.get(this);
    if (state !== undefined) return state.documentElement;
    if (documents.has(this)) return ownValue(this, "documentElement");
    throw new TypeError("receiver does not implement the test Document interface");
  }

  get defaultView() {
    const state = ambientState.get(this);
    if (state !== undefined) return state.realm;
    if (documents.has(this)) return ownValue(this, "defaultView");
    throw new TypeError("receiver does not implement the test Document interface");
  }

  createElement() {
    const state = ambientState.get(this);
    if (state === undefined) throw new TypeError("invalid test Document receiver");
    const element = new PlatformElement();
    elements.add(element);
    ambientState.set(element, {
      connected: false,
      document: this,
      root: this,
      shadowRoot: null,
      style: new PlatformStyleDeclaration(),
    });
    ambientState.set(ambientState.get(element).style, { names: [] });
    return element;
  }

  get adoptedStyleSheets() {
    const state = ambientState.get(this);
    if (state !== undefined) return state.adopted;
    if (documents.has(this)) return ownValue(this, "adoptedStyleSheets");
    throw new TypeError("receiver does not implement the test Document interface");
  }

  set adoptedStyleSheets(value) {
    const state = ambientState.get(this);
    if (state !== undefined) state.adopted = Array.from(value);
    else if (documents.has(this)) writeOwn(this, "adoptedStyleSheets", value);
    else throw new TypeError("receiver does not implement the test Document interface");
  }
}

class PlatformShadowRoot extends PlatformNode {
  get host() {
    const state = ambientState.get(this);
    if (state !== undefined) return state.host;
    if (shadowRoots.has(this)) return ownValue(this, "host");
    throw new TypeError("receiver does not implement the test ShadowRoot interface");
  }

  get mode() {
    const state = ambientState.get(this);
    if (state !== undefined) return state.mode;
    if (shadowRoots.has(this)) return ownValue(this, "mode");
    throw new TypeError("receiver does not implement the test ShadowRoot interface");
  }

  get adoptedStyleSheets() {
    const state = ambientState.get(this);
    if (state !== undefined) return state.adopted;
    if (shadowRoots.has(this)) return ownValue(this, "adoptedStyleSheets");
    throw new TypeError("receiver does not implement the test ShadowRoot interface");
  }

  set adoptedStyleSheets(value) {
    const state = ambientState.get(this);
    if (state !== undefined) state.adopted = Array.from(value);
    else if (shadowRoots.has(this)) writeOwn(this, "adoptedStyleSheets", value);
    else throw new TypeError("receiver does not implement the test ShadowRoot interface");
  }
}

const PLATFORM_PROTOTYPES = new Set([
  PlatformNode.prototype,
  PlatformStyleDeclaration.prototype,
  PlatformElement.prototype,
  PlatformDocument.prototype,
  PlatformShadowRoot.prototype,
]);

if (Object.getOwnPropertyDescriptor(globalThis, "document") !== undefined) {
  throw new Error("fake DOM oracle requires a DOM-free Node.js test realm");
}

const ambientDocument = new PlatformDocument();
const ambientElement = new PlatformElement();
const ambientStyle = new PlatformStyleDeclaration();
const ambientRealm = { CSSStyleSheet: class PlatformCSSStyleSheet {} };
documents.add(ambientDocument);
elements.add(ambientElement);
ambientState.set(ambientDocument, {
  adopted: [],
  documentElement: ambientElement,
  realm: ambientRealm,
});
ambientState.set(ambientElement, {
  connected: true,
  document: ambientDocument,
  root: ambientDocument,
  shadowRoot: null,
  style: ambientStyle,
});
ambientState.set(ambientStyle, { names: [] });
Object.defineProperty(globalThis, "document", {
  configurable: true,
  enumerable: false,
  value: ambientDocument,
  writable: false,
});

export function brandFakeDocument(document) {
  if (document === null || typeof document !== "object") {
    throw new TypeError("fake Document brand requires an object");
  }
  documents.add(document);
  return document;
}

export function brandFakeElement(element) {
  if (element === null || typeof element !== "object") {
    throw new TypeError("fake Element brand requires an object");
  }
  elements.add(element);
  return element;
}

export function brandFakeShadowRoot(root) {
  if (root === null || typeof root !== "object") {
    throw new TypeError("fake ShadowRoot brand requires an object");
  }
  shadowRoots.add(root);
  return root;
}
