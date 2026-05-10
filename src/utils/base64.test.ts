import { describe, expect, it } from "vitest";
import { uint8ToBase64 } from "./base64";

describe("uint8ToBase64", () => {
  it("encodes bytes without unsafe casts", () => {
    expect(uint8ToBase64(new Uint8Array([1, 2, 3, 252, 253, 254]))).toBe("AQID/P3+");
  });

  it("encodes payloads larger than one chunk", () => {
    const bytes = new Uint8Array(9000);
    bytes.fill(65);

    expect(atob(uint8ToBase64(bytes))).toHaveLength(9000);
  });
});
