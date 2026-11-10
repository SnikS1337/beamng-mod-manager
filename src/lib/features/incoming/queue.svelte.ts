export interface IncomingQueueItem {
  path: string;
  downloaded: boolean;
}

/** Один и тот же файл приходит то с прямыми слэшами, то с обратными — сравниваем приведённым. */
function pathKey(path: string): string {
  return path.replace(/\\/g, "/").toLowerCase();
}

class IncomingQueue {
  items = $state<IncomingQueueItem[]>([]);
  revision = $state(0);

  add(paths: string[], downloaded = false): void {
    const fresh = paths.filter(
      (path, index) =>
        paths.findIndex((other) => pathKey(other) === pathKey(path)) === index &&
        !this.items.some((item) => pathKey(item.path) === pathKey(path)),
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
