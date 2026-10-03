import { computed, shallowRef } from "vue";

type TranscriptionJobState =
  "queued" | "running" | "cancelling" | "succeeded" | "failed" | "cancelled";

type QueueJob<T> = {
  key: string;
  item: T;
  state: TranscriptionJobState;
  finished: Promise<void>;
  resolve: () => void;
  cancellation?: Promise<void>;
};

export function useTranscriptionQueue<T>(options: {
  key: (item: T) => string;
  run: (item: T) => Promise<void>;
  cancel: (item: T) => Promise<boolean>;
  onError: (error: unknown, item: T) => void;
}) {
  const jobs = shallowRef<QueueJob<T>[]>([]);
  const pending = (job: QueueJob<T>) =>
    job.state === "queued" || job.state === "running" || job.state === "cancelling";
  const active = computed(() => jobs.value.some(pending));
  const busy = computed<Record<string, boolean>>(() =>
    Object.fromEntries(jobs.value.filter(pending).map((job) => [job.key, true])),
  );
  const total = computed(() => jobs.value.length);
  const done = computed(() => jobs.value.filter((job) => !pending(job)).length);
  const states = computed(() => Object.fromEntries(jobs.value.map((job) => [job.key, job.state])));
  let running = false;

  function update(job: QueueJob<T>, state: TranscriptionJobState) {
    job.state = state;
    jobs.value = [...jobs.value];
  }

  function reportError(error: unknown, item: T) {
    try {
      options.onError(error, item);
    } catch {}
  }

  async function drain() {
    if (running) return;
    running = true;
    try {
      for (;;) {
        const job = jobs.value.find((candidate) => candidate.state === "queued");
        if (!job) break;
        update(job, "running");
        try {
          await options.run(job.item);
          if (job.cancellation) await job.cancellation;
          update(job, job.state === "cancelling" ? "cancelled" : "succeeded");
        } catch (error) {
          if (job.cancellation) await job.cancellation;
          if (job.state === "cancelling" || error === "cancelled") {
            update(job, "cancelled");
          } else {
            update(job, "failed");
            reportError(error, job.item);
          }
        } finally {
          job.resolve();
        }
      }
    } finally {
      running = false;
    }
  }

  function enqueue(items: T[]): Promise<void> {
    const next = active.value ? [...jobs.value] : [];
    const byKey = new Map(next.filter(pending).map((job) => [job.key, job]));
    const completions = items.map((item) => {
      const key = options.key(item);
      const existing = byKey.get(key);
      if (existing) return existing.finished;
      let resolve!: () => void;
      const finished = new Promise<void>((done) => (resolve = done));
      const job: QueueJob<T> = { key, item, state: "queued", finished, resolve };
      next.push(job);
      byKey.set(key, job);
      return finished;
    });
    jobs.value = next;
    void drain();
    return Promise.all(completions).then(() => undefined);
  }

  async function cancel(key: string) {
    const job = jobs.value.find((candidate) => candidate.key === key && pending(candidate));
    if (!job || job.state === "cancelling") return;
    if (job.state === "queued") {
      update(job, "cancelled");
      job.resolve();
      return;
    }
    update(job, "cancelling");
    job.cancellation = (async () => {
      try {
        const accepted = await options.cancel(job.item);
        if (!accepted && job.state === "cancelling") update(job, "running");
      } catch (error) {
        if (job.state !== "cancelling") return;
        update(job, "running");
        reportError(error, job.item);
      }
    })();
    await job.cancellation;
  }

  return { active, busy, total, done, states, enqueue, cancel };
}
