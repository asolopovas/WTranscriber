const BASE64_CHUNK_SIZE = 8192;

export function uint8ToBase64(bytes: Uint8Array): string {
  const chunks: string[] = [];
  for (let offset = 0; offset < bytes.byteLength; offset += BASE64_CHUNK_SIZE) {
    chunks.push(String.fromCharCode(...bytes.subarray(offset, offset + BASE64_CHUNK_SIZE)));
  }
  return btoa(chunks.join(""));
}
