// Interactive chat over userStream: read user input, call Chat Completion, write model output.
// 基于 userStream 的交互式对话：读取用户输入，调用 Chat Completion，并将模型输出写回用户。

import { Spear } from "spear";

function formatHelp(currentModel) {
  const m = currentModel ? String(currentModel) : "(default)";
  return [
    "Connected.",
    "Commands:",
    "  /model <name>  Set model (current: " + m + ")",
    "  /model         Show current model",
    "  /exit          Quit",
    "Send:",
    "  Type your prompt and press Send/Enter.",
    "",
  ].join("\n");
}

function normalizeInput(text) {
  return String(text).replace(/\r/g, "").trim();
}

function tryWrite(stream, text) {
  stream.sendText(String(text));
}

async function runChat({ model, prompt }) {
  const options = {
    messages: [{ role: "user", content: prompt }],
    timeoutMs: 30_000,
  };
  if (typeof model === "string" && model.trim().length > 0) {
    options.model = model.trim();
  }
  const resp = await Spear.chat.completions.create({
    ...options,
  });
  return resp.text();
}

export default async function main() {
  const ctl = Spear.userStream.ctlOpen();

  let stream = null;
  let pending = "";
  let model = null;
  let loop = 0;

  try {
    for (;;) {
      loop++;
      let didWork = false;

      const evt = ctl.readEvent();
      if (evt && typeof evt.kind === "number") {
        didWork = true;

        if (evt.kind === 1 && stream == null && typeof evt.streamId === "number") {
          stream = Spear.userStream.open(evt.streamId, Spear.userStream.Direction.BIDIRECTIONAL);
          tryWrite(stream, formatHelp(model));
        } else if (evt.kind === 2) {
          break;
        }
      }

      if (stream) {
        const msg = stream.readMessage();
        if (msg) {
          didWork = true;

          if (msg.kind === "data") {
            if (msg.text) pending += msg.text;
          } else if (msg.kind === "commit") {
            const inputText = pending;
            pending = "";

            const prompt = normalizeInput(inputText);
            if (!prompt) continue;
            if (prompt === "/exit") return "bye";
            if (prompt === "/model") {
              tryWrite(stream, "current model: " + (model ? String(model) : "(default)") + "\n");
              continue;
            }
            if (prompt.startsWith("/model ")) {
              const next = prompt.slice("/model ".length).trim();
              model = next.length > 0 ? next : null;
              tryWrite(stream, "model set to: " + (model ? String(model) : "(default)") + "\n");
              continue;
            }

            try {
              const answer = await runChat({ model, prompt });
              tryWrite(stream, answer + "\n");
            } catch (e) {
              tryWrite(stream, "error: " + String(e) + "\n");
            }
          }
        }
      }

      if (!didWork) {
        Spear.sleepMs(10);
      }
    }
  } finally {
    try {
      stream?.close();
    } catch (_) {}
    try {
      ctl.close();
    } catch (_) {}
  }

  return "done";
}
