export const OUTPUT_BINDING = "--lab-private-program-output";
export const EXPECTED_OUTPUT = 17;
export const EXPECTED_SINK_OUTPUT = 501;
export const REQUEST_HEX = "4c43465102004600404040000000000000e03f00000000000050409a9999999999c93f01606060f50100001f0000000100000000000000010100000080808000000000000000";
export const UPDATE_HEX = "4c434655020028001f00000002000000000000000101020000008080800000000000000000000000";
export const CHANGED_UPDATE_HEX = "4c434655020028001f0000000300000000000000010103000000ffffff0000000000000000000000";
export const EXPECTED_CONTENT_IDENTITY = "038cc3793075d1855c4ebfc03b542e34ed4d23e23e54f6a26ce026905b26677a";

export function exactWireBytes(hex) {
  if (!/^(?:[0-9a-f]{2})+$/u.test(hex)) throw new TypeError("wire vector must be lowercase hex");
  const bytes = new Uint8Array(hex.length / 2);
  for (let index = 0; index < bytes.length; index += 1) {
    bytes[index] = Number.parseInt(hex.slice(index * 2, index * 2 + 2), 16);
  }
  return bytes;
}

export function receiptFingerprint(receipt) {
  return JSON.stringify({
    output: receipt.output,
    sinkOutput: receipt.sinkOutput,
    state: receipt.state,
    stream: receipt.stream,
    revision: receipt.revision.toString(),
    paintSource: receipt.paintSource,
    paintOpacityBits: receipt.paintOpacityBits.toString(16).padStart(16, "0"),
    contentIdentity: receipt.contentIdentity,
  });
}
