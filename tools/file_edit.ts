import * as fs from "fs";
export default function ({
  path,
  find,
  replace,
}: {
  path: string;
  find: string;
  replace: string;
}) {
  fs.writeFileSync(path, fs.readFileSync(path, "utf-8").replace(find, replace));
  return { success: true };
}
