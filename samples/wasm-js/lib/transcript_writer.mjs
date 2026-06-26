import { currentHmsPrefix } from "spear/time_format";

export class TranscriptWriter {
  constructor(textWriter) {
    this.textWriter = textWriter;
    this.lineOpen = false;
  }

  beginLine() {
    if (this.lineOpen) return;
    this.textWriter.write(currentHmsPrefix());
    this.textWriter.flush();
    this.lineOpen = true;
  }

  appendDelta(text) {
    if (!text) return;
    if (!this.lineOpen) {
      this.beginLine();
    }
    this.textWriter.write(text);
    this.textWriter.flush();
  }

  completeLine() {
    if (!this.lineOpen) return false;
    this.textWriter.write("\n");
    this.textWriter.commit();
    this.textWriter.flush();
    this.lineOpen = false;
    return true;
  }
}
