export type DeliverProactiveArgs = {
  content: string;
  ttsEnabled: boolean;
  append: (content: string) => void;
  speak: (text: string) => Promise<unknown>;
};

/** Append a proactive nudge; speak it only when voice-output is on. */
export function deliverProactive({
  content,
  ttsEnabled,
  append,
  speak,
}: DeliverProactiveArgs): void {
  append(content);
  if (ttsEnabled && content.trim()) {
    void Promise.resolve(speak(content)).catch(() => {
      /* TTS failure must not affect text delivery */
    });
  }
}
