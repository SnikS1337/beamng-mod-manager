export interface IncomingQueueItem {
  path: string;
  downloaded: boolean;
}

class IncomingQueue {
  items = $state<IncomingQueueItem[]>([]);
  revision = $state(0);

  add(paths: string[], downloaded = false): void {
    const fresh = paths.filter(
      (path, index) =>
        paths.indexOf(path) === index &&
        !this.items.some((item) => item.path === path),
    );
    if (fresh.length === 0) return;
    this.items.push(...fresh.map((path) => ({ path, downloaded })));
    this.revision += 1;
  }

  shift(): IncomingQueueItem | undefined {
    return this.items.shift();
  }

  get count(): number {
    return this.items.length;
  }
}

export const incomingQueue = new IncomingQueue();
