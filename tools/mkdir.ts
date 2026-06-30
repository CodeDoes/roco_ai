import * as fs from "fs";
export default function ({ path }: { path: string }) {
  fs.mkdirSync(path, { recursive: true });
  return { success: true };
}
