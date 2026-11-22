import { authoringError } from "./authoring.js";

const roots = new WeakSet();
const attribute = "data-lab-colors-scope";
let nextScope = 0;

/** One owned stylesheet record is the complete frame, including controlled surface inputs. */
export function createCssScope(element) {
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
  const token = `colors-${++nextScope}`;
  const style = document.createElement("style");
  style.setAttribute("data-lab-colors", token);
  const setText = Object.getOwnPropertyDescriptor(realm.Node.prototype, "textContent").set;
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
      if (disposed || style.parentNode !== parent || element.getRootNode() !== root
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
      return `[${attribute}="${token}"]{${values.map(([name, css]) => `--${name}:${css};`).join("")}}`;
    },
    install(prepared) {
      // The native setter replaces the owned text node in one DOM mutation.
      // All parsing, value copies, string creation and host checks precede this effect.
      Reflect.apply(setText, style, [prepared]);
      text = prepared;
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
