import { describe, expect, it } from "vitest";
import { useFileSelection } from "./useFileSelection";

const ALL = ["/a", "/b", "/c", "/d", "/e"];
const all = () => ALL;

describe("useFileSelection", () => {
  it("starts empty", () => {
    const sel = useFileSelection();
    expect(sel.size()).toBe(0);
    expect(sel.hasSelection()).toBe(false);
    expect(sel.asArray()).toEqual([]);
  });

  it("toggle adds and removes a path", () => {
    const sel = useFileSelection();
    sel.toggle("/a");
    expect(sel.asArray()).toEqual(["/a"]);
    sel.toggle("/a");
    expect(sel.asArray()).toEqual([]);
  });

  it("toggle replaces the set reference (so Vue reactivity fires)", () => {
    const sel = useFileSelection();
    const before = sel.selected.value;
    sel.toggle("/a");
    expect(sel.selected.value).not.toBe(before);
  });

  it("range from a previous anchor selects the inclusive span", () => {
    const sel = useFileSelection();
    sel.toggle("/b");
    sel.range("/d", all);
    expect(sel.asArray().sort()).toEqual(["/b", "/c", "/d"]);
  });

  it("range without prior anchor falls back to single selection", () => {
    const sel = useFileSelection();
    sel.range("/c", all);
    expect(sel.asArray()).toEqual(["/c"]);
  });

  it("range works backwards too", () => {
    const sel = useFileSelection();
    sel.toggle("/d");
    sel.range("/b", all);
    expect(sel.asArray().sort()).toEqual(["/b", "/c", "/d"]);
  });

  it("range ignores unknown paths", () => {
    const sel = useFileSelection();
    sel.toggle("/a");
    sel.range("/missing", all);
    expect(sel.asArray()).toEqual(["/a"]);
  });

  it("range '__all__' toggles select-all/clear", () => {
    const sel = useFileSelection();
    sel.range("__all__", all);
    expect(sel.asArray()).toEqual(ALL);
    sel.range("__all__", all);
    expect(sel.asArray()).toEqual([]);
  });

  it("clear empties the set and forgets the anchor", () => {
    const sel = useFileSelection();
    sel.toggle("/b");
    sel.clear();
    expect(sel.asArray()).toEqual([]);
    sel.range("/d", all);
    expect(sel.asArray()).toEqual(["/d"]);
  });
});
