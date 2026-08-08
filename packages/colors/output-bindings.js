const CANONICAL_OUTPUT_BINDING_NAME = /^--[a-z0-9-]+$/u;

export function isCanonicalOutputBindingName(value) {
  return typeof value === "string" && CANONICAL_OUTPUT_BINDING_NAME.test(value);
}

export function outputBindingsEqual(left, right) {
  return left.length === right.length && left.every((name, index) => name === right[index]);
}
