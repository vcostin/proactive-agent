import { assertEquals } from "jsr:@std/assert@1";
import { deliverProactive } from "./deliverProactive.ts";

Deno.test("voice on: appends proactive text and speaks it", () => {
  const spoken: string[] = [];
  const appended: string[] = [];

  deliverProactive({
    content: "Time to stretch",
    ttsEnabled: true,
    append: (content: string) => appended.push(content),
    speak: (text: string) => {
      spoken.push(text);
      return Promise.resolve();
    },
  });

  assertEquals(appended, ["Time to stretch"]);
  assertEquals(spoken, ["Time to stretch"]);
});

Deno.test("voice off: appends proactive text only", () => {
  const spoken: string[] = [];
  const appended: string[] = [];

  deliverProactive({
    content: "Time to stretch",
    ttsEnabled: false,
    append: (content: string) => appended.push(content),
    speak: (text: string) => {
      spoken.push(text);
      return Promise.resolve();
    },
  });

  assertEquals(appended, ["Time to stretch"]);
  assertEquals(spoken, []);
});

Deno.test("speak failure leaves text delivery intact", async () => {
  const appended: string[] = [];
  let speakCalls = 0;

  deliverProactive({
    content: "Still show me",
    ttsEnabled: true,
    append: (content: string) => appended.push(content),
    speak: () => {
      speakCalls += 1;
      return Promise.reject(new Error("piper missing"));
    },
  });

  assertEquals(appended, ["Still show me"]);
  assertEquals(speakCalls, 1);
  // Let the rejected promise settle through the helper's catch
  await Promise.resolve();
  await Promise.resolve();
});
