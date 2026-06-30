import * as fs from "fs";
import * as path from "path";
export default function ({ path: dir, term }: { path: string; term: string }) {
  let dirs: string[] = [dir];
  let results: string[] = [];
  while (dirs.length > 0) {
    const current = dirs.splice(0, 1)[0];
    for (const entry of fs.readdirSync(current)) {
      const full = path.join(current, entry);
      if (fs.statSync(full).isDirectory()) {
        dirs.push(full);
      }
      if (entry.includes(term)) {
        results.push(full);
      }
    }
  }
  return results;
}
