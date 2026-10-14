// Pure helpers for locating the project root and the commands the extension runs.
import * as fs from "fs";
import * as path from "path";

/** Nearest ancestor folder of `file` that holds an agentmark.toml; otherwise the file itself. */
export function projectTarget(file: string, exists: (p: string) => boolean = fs.existsSync): string {
  let dir = path.dirname(file);
  for (;;) {
    if (exists(path.join(dir, "agentmark.toml"))) return dir;
    const parent = path.dirname(dir);
    if (parent === dir) return file;
    dir = parent;
  }
}

export interface Invocation {
  command: string;
  args: string[];
  cwd: string;
}

export function checkInvocation(cli: string, target: string): Invocation {
  const isDir = (() => {
    try {
      return fs.statSync(target).isDirectory();
    } catch {
      return false;
    }
  })();
  return { command: cli, args: ["check", "--json", target], cwd: isDir ? target : path.dirname(target) };
}
