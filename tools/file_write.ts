import * as fs from "fs";
export default function ({ path, content }: { path: string; content: string }) {
  return fs.writeFileSync(path, content);
}
