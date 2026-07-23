import * as path from 'path';
import * as fs from 'fs';

export class Sandbox {
  private root: string;
  private allowedExts: string[];

  constructor(root: string) {
    this.root = path.resolve(root);
    this.allowedExts = ['txt', 'md', 'json', 'py', 'rs'];
  }

  private resolvePath(subPath: string, isDelete = false): string {
    const resolved = path.resolve(this.root, subPath);
    const relative = path.relative(this.root, resolved);
    if (relative.startsWith('..') || path.isAbsolute(relative)) {
      throw new Error(isDelete ? 'escape' : 'path escape blocked');
    }
    return resolved;
  }

  read(subPath: string): string {
    const full = this.resolvePath(subPath);
    if (!fs.existsSync(full)) {
      throw new Error('file not found');
    }
    const stat = fs.statSync(full);
    if (stat.size > 10_000_000) {
      throw new Error('file too large');
    }
    try {
      return fs.readFileSync(full, 'utf8');
    } catch {
      throw new Error('read error');
    }
  }

  write(subPath: string, content: string): void {
    const full = this.resolvePath(subPath);
    try {
      const parent = path.dirname(full);
      fs.mkdirSync(parent, { recursive: true });
      fs.writeFileSync(full, content, 'utf8');
    } catch {
      throw new Error('write error');
    }
  }

  allowed(subPath: string): boolean {
    return this.allowedExts.some(ext => subPath.endsWith(ext));
  }

  listFiles(): string[] {
    if (!fs.existsSync(this.root)) {
      return [];
    }
    try {
      const results = fs.readdirSync(this.root);
      results.sort();
      return results;
    } catch {
      return [];
    }
  }

  exists(subPath: string): boolean {
    try {
      const full = this.resolvePath(subPath);
      return fs.existsSync(full);
    } catch {
      return false;
    }
  }

  delete(subPath: string): void {
    const full = this.resolvePath(subPath, true);
    try {
      if (!fs.existsSync(full)) {
        throw new Error();
      }
      fs.rmSync(full);
    } catch {
      throw new Error('delete failed');
    }
  }

  sizeLimitCheck(subPath: string, limit: number): boolean {
    try {
      const full = this.resolvePath(subPath);
      const stat = fs.statSync(full);
      return stat.size <= limit;
    } catch {
      return false;
    }
  }
}
