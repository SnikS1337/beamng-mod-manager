class IncomingQueue {
  paths = $state<string[]>([]);
  revision = $state(0);

  add(paths: string[]): void {
    const fresh = paths.filter((path, index) => paths.indexOf(path) === index);
    if (fresh.length === 0) return;
    this.paths.push(...fresh);
    this.revision += 1;
  }

  shift(): string | undefined {
    return this.paths.shift();
  }

  get count(): number {
    return this.paths.length;
  }
}

export const incomingQueue = new IncomingQueue();
