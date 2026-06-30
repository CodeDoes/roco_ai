import * as fs from "fs";
export default function ({ path }: { path: string }) {
  return fs.readdirSync(path);
}
