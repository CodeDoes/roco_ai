import * as fs from "fs";
export default function ({ path }: { path: string }) {
  if (path.includes("#")) {
    const [fromLine, toLine] = path.split("#")[1].split(":").map(parseInt);
    path = path.split("#")[0];
    return fs.readFileSync(path, "utf-8").split("\n").slice(fromLine, toLine);
  }
  return fs.readFileSync(path, "utf-8");
}
