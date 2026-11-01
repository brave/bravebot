// What a picture a tool returns has to be: a JPEG as base64 within a size. Both
// places that pass a picture on use this, screenshot_page for the picture the
// browser takes of a tab (tools.js) and the tool of capture-trusted for the one
// the BraveBot Trusted Input extension makes (trusted.js).

// The largest picture a call returns, in bytes once decoded. It is the largest
// picture the MCP client takes from a tool (MAX_PICTURE_BYTES in
// crates/mcp/src/protocol.rs), so a larger one is refused here, saying why,
// where it would be left out there with a count. extension-trusted/capture.js
// holds the same number.
export const MAX_PICTURE_BYTES = 8 * 1024 * 1024;

// The media type of the pictures that are passed on.
export const PICTURE_TYPE = "image/jpeg";

// Whether `data` is a non-empty string in the canonical form of base64: the
// standard alphabet, a length that is a multiple of four, and padding only at
// the end.
export function isBase64(data) {
  return (
    typeof data === "string" &&
    data.length % 4 === 0 &&
    /^[A-Za-z0-9+/]+={0,2}$/.test(data)
  );
}

// The number of bytes a base64 string decodes to.
export function decodedSize(base64) {
  const padding = base64.endsWith("==") ? 2 : base64.endsWith("=") ? 1 : 0;
  return (base64.length / 4) * 3 - padding;
}
