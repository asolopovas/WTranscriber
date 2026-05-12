#!/usr/bin/env bun
import { openSync, closeSync, statSync, readSync, existsSync } from "node:fs";

const path = process.argv[2];
if (!path) {
  process.stderr.write("usage: dev-tail.ts <file>\n");
  process.exit(2);
}

let offset = 0;
if (existsSync(path)) offset = statSync(path).size;

while (true) {
  if (existsSync(path)) {
    const st = statSync(path);
    if (st.size < offset) offset = 0;
    if (st.size > offset) {
      const fd = openSync(path, "r");
      const len = st.size - offset;
      const buf = Buffer.alloc(len);
      readSync(fd, buf, 0, len, offset);
      closeSync(fd);
      offset = st.size;
      process.stdout.write(buf);
    }
  }
  await Bun.sleep(250);
}
