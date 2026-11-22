import { authoringError } from "./authoring.js";

const roots = new WeakSet();
const attribute = "data-lab-colors-scope";
const scopeAllocator = Symbol.for("@labpics/colors.css-scope.v1");

function allocateScope(document) {
  // Independently loaded copies share the same document and must never share a selector.
  if (!Object.hasOwn(document, scopeAllocator)) {
    let next = 0n;
    Object.defineProperty(document, scopeAllocator, { value: () => `colors-${++next}` });
  }
  return document[scopeAllocator]();
}

function readNonce(options) {
  if (options === undefined) return undefined;
  let keys, nonce;
  try {
    if (options === null || typeof options !== "object" || Array.isArray(options)) {
      throw new TypeError();
    }
    keys = Object.keys(options);
    nonce = Object.hasOwn(options, "nonce") ? options.nonce : undefined;
  } catch {
    authoringError("attach.options", "expected readable options", "color_invalid_context");
  }
  for (const key of keys) {
    if (key !== "nonce") authoringError(`attach.options.${key}`, "unknown option", "color_invalid_context");
  }
  if (nonce !== undefined && (typeof nonce !== "string" || nonce.length === 0)) {
    authoringError("attach.options.nonce", "expected a non-empty CSP nonce", "color_invalid_context");
  }
  return nonce;
}

/** One owned stylesheet record is the complete frame, including controlled surface inputs. */
export function createCssScope(element, options) {
  const nonce = readNonce(options);
  const document = element?.ownerDocument, realm = document?.defaultView;
  if (!realm || !(element instanceof realm.HTMLElement) || !element.isConnected) {
    authoringError("attach.element", "expected a connected HTMLElement", "color_invalid_context");
  }
  if (roots.has(element) || element.hasAttribute(attribute)) {
    authoringError("attach.element", "element already has a color owner", "color_host_changed");
  }
  const root = element.getRootNode();
  const parent = root === document ? document.head : root;
  if (parent === null || typeof parent.appendChild !== "function") {
    authoringError("attach.element", "element has no stylesheet host", "color_invalid_context");
  }
  const token = allocateScope(document);
  const style = document.createElement("style");
  style.setAttribute("data-lab-colors", token);
  if (nonce !== undefined) style.nonce = nonce;
  const setText = Object.getOwnPropertyDescriptor(realm.Node.prototype, "textContent").set;
  const getComputedStyle = realm.getComputedStyle;
  const removeChild = realm.Node.prototype.removeChild;
  const removeAttribute = realm.Element.prototype.removeAttribute;
  let text = "", disposed = false;
  roots.add(element);
  try {
    element.setAttribute(attribute, token);
    parent.appendChild(style);
  } catch (error) {
    if (style.parentNode) Reflect.apply(removeChild, style.parentNode, [style]);
    if (element.getAttribute(attribute) === token) Reflect.apply(removeAttribute, element, [attribute]);
    roots.delete(element);
    throw error;
  }
  return {
    prepare(context, frame) {
      if (disposed || !element.isConnected || style.parentNode !== parent || element.getRootNode() !== root
        || element.getAttribute(attribute) !== token || style.textContent !== text) {
        authoringError("view", "owned CSS scope was changed externally", "color_host_changed");
      }
      const values = [...context.colors.map(({ name, css }) => [name, css]),
        ...Object.entries(frame.values).map(([name, value]) => [name, value.css])];
      for (const [name] of values) {
        if (element.style.getPropertyValue(`--${name}`) !== "") {
          authoringError(`view.--${name}`, "an inline value overrides an owned variable", "color_host_changed");
        }
      }
      return { values, text: `[${attribute}="${token}"]{${values.map(([name, css]) => `--${name}:${css};`).join("")}}` };
    },
    install(prepared) {
      // The native setter replaces the owned text node in one DOM mutation.
      // CSS acceptance is checked before confirming the Core transaction. A blocked
      // sheet or a stronger cascade rolls back within this task, before a browser paint.
      try {
        Reflect.apply(setText, style, [prepared.text]);
        if (style.sheet === null || style.sheet.disabled) {
          authoringError("view.stylesheet", "the browser blocked or disabled the owned stylesheet", "color_host_changed");
        }
        const computed = Reflect.apply(getComputedStyle, realm, [element]);
        for (const [name, css] of prepared.values) {
          if (computed.getPropertyValue(`--${name}`).trim() !== css) {
            authoringError(`view.--${name}`, "the CSS cascade did not apply the owned value", "color_host_changed");
          }
        }
        text = prepared.text;
      } catch (error) {
        if (style.textContent !== text) Reflect.apply(setText, style, [text]);
        throw error;
      }
    },
    dispose() {
      if (disposed) return;
      if (style.parentNode) Reflect.apply(removeChild, style.parentNode, [style]);
      if (element.getAttribute(attribute) === token) Reflect.apply(removeAttribute, element, [attribute]);
      roots.delete(element);
      disposed = true;
    },
  };
}
