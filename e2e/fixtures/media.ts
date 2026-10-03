import type { Page } from "@playwright/test";

declare global {
  interface Window {
    __WT_MEDIA__: { requestedAudio: boolean; stoppedTracks: number };
  }
}

export async function installSyntheticRecording(page: Pick<Page, "addInitScript">) {
  await page.addInitScript(() => {
    window.__WT_MEDIA__ = { requestedAudio: false, stoppedTracks: 0 };
    Object.defineProperty(navigator, "mediaDevices", {
      configurable: true,
      value: {
        getUserMedia: async (constraints: MediaStreamConstraints) => {
          window.__WT_MEDIA__.requestedAudio = constraints.audio === true;
          return { getTracks: () => [{ stop: () => window.__WT_MEDIA__.stoppedTracks++ }] };
        },
      },
    });

    class SyntheticRecorder {
      static isTypeSupported(type: string) {
        return type.startsWith("audio/webm");
      }
      state = "inactive";
      mimeType = "audio/webm";
      ondataavailable: ((event: BlobEvent) => void) | null = null;
      onstop: ((event: Event) => unknown) | null = null;
      start() {
        this.state = "recording";
      }
      stop() {
        this.state = "inactive";
        this.ondataavailable?.(new BlobEvent("dataavailable", { data: new Blob(["synthetic"]) }));
        queueMicrotask(() => this.onstop?.(new Event("stop")));
      }
    }

    class SyntheticAudioContext {
      createAnalyser() {
        return {
          fftSize: 256,
          frequencyBinCount: 128,
          getByteFrequencyData: (bins: Uint8Array) => bins.fill(64),
        };
      }
      createMediaStreamSource() {
        return { connect: () => undefined };
      }
      async decodeAudioData() {
        return {
          numberOfChannels: 2,
          length: 480,
          sampleRate: 48000,
          getChannelData: (channel: number) =>
            new Float32Array(480).fill(channel === 0 ? 0.5 : -0.25),
        };
      }
      async close() {}
    }

    Object.defineProperty(window, "MediaRecorder", {
      configurable: true,
      value: SyntheticRecorder,
    });
    Object.defineProperty(window, "AudioContext", {
      configurable: true,
      value: SyntheticAudioContext,
    });
  });
}
