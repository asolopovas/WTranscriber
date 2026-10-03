import { describe, expect, it, vi } from "vitest";
import { useTranscriptionQueue } from "./useTranscriptionQueue";

function deferred() {
  let resolve!: () => void;
  let reject!: (error: unknown) => void;
  const promise = new Promise<void>((res, rej) => {
    resolve = res;
    reject = rej;
  });
  return { promise, resolve, reject };
}

const flush = async () => {
  for (let i = 0; i < 8; i += 1) await Promise.resolve();
};

function harness() {
  const work = new Map<string, ReturnType<typeof deferred>>();
  let active = 0;
  let peak = 0;
  const run = vi.fn(async (key: string) => {
    active += 1;
    peak = Math.max(peak, active);
    const task = deferred();
    work.set(key, task);
    try {
      await task.promise;
    } finally {
      active -= 1;
    }
  });
  const cancel = vi.fn(async (_key: string) => true);
  const onError = vi.fn();
  const queue = useTranscriptionQueue({ key: (key: string) => key, run, cancel, onError });
  return { queue, work, run, cancel, onError, peak: () => peak };
}

describe("useTranscriptionQueue", () => {
  it("appends remaining files without interrupting the active file and drains FIFO at one job", async () => {
    const { queue, work, run, cancel, peak } = harness();
    expect(queue.active.value).toBe(false);
    expect(queue.states.value).toEqual({});
    const first = queue.enqueue(["a"]);
    const remaining = queue.enqueue(["b", "c"]);
    expect(queue.states.value).toEqual({ a: "running", b: "queued", c: "queued" });
    expect(queue.busy.value).toEqual({ a: true, b: true, c: true });
    expect(queue.total.value).toBe(3);
    expect(queue.done.value).toBe(0);
    expect(cancel).not.toHaveBeenCalled();
    expect(run.mock.calls).toEqual([["a"]]);

    work.get("a")!.resolve();
    await first;
    expect(queue.states.value).toEqual({ a: "succeeded", b: "running", c: "queued" });
    expect(queue.done.value).toBe(1);
    work.get("b")!.resolve();
    await flush();
    expect(queue.states.value.c).toBe("running");
    work.get("c")!.resolve();
    await remaining;
    expect(queue.active.value).toBe(false);
    expect(queue.busy.value).toEqual({});
    expect(queue.done.value).toBe(3);
    expect(peak()).toBe(1);
    expect(run.mock.calls).toEqual([["a"], ["b"], ["c"]]);
  });

  it("deduplicates running and queued files across overlapping submissions", async () => {
    const { queue, run, work } = harness();
    const first = queue.enqueue(["a", "b", "b"]);
    const second = queue.enqueue(["a", "b", "c"]);
    expect(queue.total.value).toBe(3);
    for (const key of ["a", "b", "c"]) {
      work.get(key)!.resolve();
      await flush();
    }
    await Promise.all([first, second]);
    expect(run.mock.calls).toEqual([["a"], ["b"], ["c"]]);
  });

  it("continues after a job fails and after an independently cancelled job", async () => {
    const { queue, work, onError } = harness();
    const completion = queue.enqueue(["a", "b", "c"]);
    work.get("a")!.reject(new Error("engine failed"));
    await flush();
    expect(queue.states.value).toEqual({ a: "failed", b: "running", c: "queued" });
    expect(onError).toHaveBeenCalledExactlyOnceWith(new Error("engine failed"), "a");
    work.get("b")!.reject("cancelled");
    await flush();
    expect(queue.states.value.c).toBe("running");
    expect(queue.states.value.b).toBe("cancelled");
    work.get("c")!.resolve();
    await completion;
    expect(queue.done.value).toBe(3);
    expect(onError).toHaveBeenCalledTimes(1);
  });

  it("cancels a queued file without native cancellation or affecting other files", async () => {
    const { queue, cancel, run, work } = harness();
    const completion = queue.enqueue(["a", "b", "c"]);
    await queue.cancel("b");
    expect(cancel).not.toHaveBeenCalled();
    expect(queue.states.value).toEqual({ a: "running", b: "cancelled", c: "queued" });
    expect(queue.done.value).toBe(1);
    work.get("a")!.resolve();
    await flush();
    work.get("c")!.resolve();
    await completion;
    expect(run.mock.calls).toEqual([["a"], ["c"]]);
  });

  it("retains the active slot while cancellation drains and accepts new jobs", async () => {
    const { queue, cancel, run, work, peak } = harness();
    const first = queue.enqueue(["a", "b"]);
    await queue.cancel("a");
    await queue.cancel("a");
    const appended = queue.enqueue(["c"]);
    expect(cancel).toHaveBeenCalledExactlyOnceWith("a");
    expect(queue.states.value).toEqual({ a: "cancelling", b: "queued", c: "queued" });
    expect(run).toHaveBeenCalledTimes(1);
    expect(queue.active.value).toBe(true);
    work.get("a")!.reject("cancelled");
    await flush();
    expect(queue.states.value.a).toBe("cancelled");
    expect(queue.states.value.b).toBe("running");
    work.get("b")!.resolve();
    await first;
    work.get("c")!.resolve();
    await appended;
    expect(peak()).toBe(1);
  });

  it("keeps successful work running when native cancellation is no longer available", async () => {
    const { queue, cancel, work } = harness();
    cancel.mockResolvedValueOnce(false);
    const completion = queue.enqueue(["a", "b"]);
    await queue.cancel("a");
    expect(queue.states.value).toEqual({ a: "running", b: "queued" });
    work.get("a")!.resolve();
    await flush();
    expect(queue.states.value.a).toBe("succeeded");
    work.get("b")!.resolve();
    await completion;
  });

  it("settles cancellation acknowledgement before classifying work that already finished", async () => {
    const { queue, cancel, work, run } = harness();
    const acknowledgement = deferred();
    cancel.mockImplementationOnce(async () => {
      await acknowledgement.promise;
      return false;
    });
    const completion = queue.enqueue(["a", "b"]);
    const cancellation = queue.cancel("a");
    work.get("a")!.resolve();
    await flush();
    expect(queue.states.value.a).toBe("cancelling");
    expect(run).toHaveBeenCalledTimes(1);
    acknowledgement.resolve();
    await cancellation;
    await flush();
    expect(queue.states.value).toEqual({ a: "succeeded", b: "running" });
    work.get("b")!.resolve();
    await completion;
  });

  it("restores an active job after cancellation fails without losing the queue", async () => {
    const { queue, cancel, work, onError } = harness();
    cancel.mockRejectedValueOnce(new Error("IPC failed"));
    const completion = queue.enqueue(["a", "b"]);
    await queue.cancel("a");
    expect(queue.states.value.a).toBe("running");
    expect(onError).toHaveBeenCalledExactlyOnceWith(new Error("IPC failed"), "a");
    work.get("a")!.resolve();
    await flush();
    work.get("b")!.resolve();
    await completion;
    expect(queue.states.value).toEqual({ a: "succeeded", b: "succeeded" });
  });

  it("continues after a runner throws before returning a promise", async () => {
    const { queue, run, work, onError } = harness();
    run.mockImplementationOnce(() => {
      throw new Error("cannot start");
    });
    const completion = queue.enqueue(["a", "b"]);
    expect(queue.states.value).toEqual({ a: "failed", b: "running" });
    work.get("b")!.resolve();
    await completion;
    expect(onError).toHaveBeenCalledExactlyOnceWith(new Error("cannot start"), "a");
  });

  it("allows a cancelled queued path to be added again while another file runs", async () => {
    const { queue, work, run } = harness();
    const first = queue.enqueue(["a", "b"]);
    await queue.cancel("b");
    const retry = queue.enqueue(["b"]);
    expect(queue.states.value).toEqual({ a: "running", b: "queued" });
    expect(queue.total.value).toBe(3);
    expect(queue.done.value).toBe(1);
    work.get("a")!.resolve();
    await first;
    work.get("b")!.resolve();
    await retry;
    expect(run.mock.calls).toEqual([["a"], ["b"]]);
    expect(queue.done.value).toBe(3);
  });

  it("continues draining when an error observer throws", async () => {
    const { queue, work, onError } = harness();
    onError.mockImplementation(() => {
      throw new Error("observer failed");
    });
    const completion = queue.enqueue(["a", "b"]);
    work.get("a")!.reject(new Error("engine failed"));
    await flush();
    expect(queue.states.value).toEqual({ a: "failed", b: "running" });
    work.get("b")!.resolve();
    await completion;
    expect(queue.active.value).toBe(false);
  });

  it("treats errors mentioning cancellation as failures unless they match the IPC marker", async () => {
    const { queue, work, onError } = harness();
    const completion = queue.enqueue(["a"]);
    work.get("a")!.reject("transcribe: model download cancelled unexpectedly");
    await completion;
    expect(queue.states.value.a).toBe("failed");
    expect(onError).toHaveBeenCalledTimes(1);
  });

  it("starts a fresh batch after settling and allows retrying a failed path", async () => {
    const { queue, work } = harness();
    const first = queue.enqueue(["a"]);
    work.get("a")!.reject("failed");
    await first;
    const retry = queue.enqueue(["a", "b"]);
    expect(queue.total.value).toBe(2);
    expect(queue.done.value).toBe(0);
    expect(queue.states.value).toEqual({ a: "running", b: "queued" });
    work.get("a")!.resolve();
    await flush();
    work.get("b")!.resolve();
    await retry;
    await queue.cancel("missing");
    expect(queue.active.value).toBe(false);
  });
});
