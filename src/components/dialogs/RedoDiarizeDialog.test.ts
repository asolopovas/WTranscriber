import { mount } from "@vue/test-utils";
import { describe, expect, it } from "vitest";
import type { SystemInfo } from "@/types";
import RedoDiarizeDialog from "./RedoDiarizeDialog.vue";

const sys = (overrides: Partial<SystemInfo>): SystemInfo => ({
  os: "linux",
  arch: "x86_64",
  cpu_threads: 8,
  is_mobile: false,
  cuda_available: false,
  nnapi_available: false,
  app_version: "test",
  workdir: null,
  models_dir: null,
  cache_dir: null,
  config_dir: null,
  total_memory_bytes: 1,
  ...overrides,
});

describe("RedoDiarizeDialog", () => {
  it("shows both diarizers on desktop", () => {
    const wrapper = mount(RedoDiarizeDialog, {
      props: {
        sys: sys({}),
        open: true,
        diarizer: "titanet",
        speakers: 0,
      },
    });

    const options = wrapper
      .findAll("select")[0]!
      .findAll("option")
      .map((option) => option.text());

    expect(options).toContain("NVIDIA Sortformer v2.1 (ONNX, ≤4 speakers)");
    expect(options).toContain("pyannote-3.0 + TitaNet-Large (>4 speakers)");
  });

  it("exposes the same diarizers on mobile", () => {
    const wrapper = mount(RedoDiarizeDialog, {
      props: {
        sys: sys({ os: "android", is_mobile: true }),
        open: true,
        diarizer: "titanet",
        speakers: 0,
      },
    });

    const values = wrapper
      .findAll("select")[0]!
      .findAll("option")
      .map((option) => option.attributes("value"));

    expect(values).toEqual(["sortformer-onnx", "titanet"]);
  });

  it("clamps speaker count when switching to a four-speaker diarizer", async () => {
    const wrapper = mount(RedoDiarizeDialog, {
      props: {
        sys: sys({}),
        open: true,
        diarizer: "titanet",
        speakers: 8,
      },
    });

    await wrapper.findAll("select")[0]!.setValue("sortformer-onnx");

    const diarizerEvents = wrapper.emitted("update:diarizer") ?? [];
    const speakerEvents = wrapper.emitted("update:speakers") ?? [];
    expect(diarizerEvents[diarizerEvents.length - 1]).toEqual(["sortformer-onnx"]);
    expect(speakerEvents[speakerEvents.length - 1]).toEqual([4]);
  });
});
