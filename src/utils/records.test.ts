import { describe, expect, it } from "vitest";
import { ref } from "vue";
import { recordOmit, recordSet } from "./records";

describe("recordSet", () => {
  it("adds a new key without mutating the previous object", () => {
    const target = ref<Record<string, number>>({ a: 1 });
    const before = target.value;
    recordSet(target, "b", 2);
    expect(target.value).toEqual({ a: 1, b: 2 });
    expect(target.value).not.toBe(before);
  });

  it("overwrites an existing key", () => {
    const target = ref<Record<string, number>>({ a: 1 });
    recordSet(target, "a", 9);
    expect(target.value).toEqual({ a: 9 });
  });
});

describe("recordOmit", () => {
  it("removes a key and creates a new object", () => {
    const target = ref<Record<string, number>>({ a: 1, b: 2 });
    const before = target.value;
    recordOmit(target, "a");
    expect(target.value).toEqual({ b: 2 });
    expect(target.value).not.toBe(before);
  });

  it("is a no-op when the key is missing", () => {
    const target = ref<Record<string, number>>({ a: 1 });
    const before = target.value;
    recordOmit(target, "missing");
    expect(target.value).toBe(before);
  });
});
