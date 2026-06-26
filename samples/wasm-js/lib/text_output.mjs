import { currentHmsPrefix } from "spear/time_format";

export class TextOutputWriter {
  constructor(output, meta = { v: 1 }) {
    this.output = output;
    this.meta = meta;
  }

  write(text) {
    this.output.write(text, this.meta);
  }

  writeLine(text) {
    this.write(text);
    this.write("\n");
  }

  commit() {
    this.output.commit(this.meta);
  }

  flush() {
    this.output.flush();
  }

  writeTimestampedLine(text) {
    this.write(currentHmsPrefix());
    this.writeLine(text);
  }
}
