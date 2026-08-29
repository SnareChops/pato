import { bind } from "./bindings";

const version = 3;

let db: IDBDatabase;

type Index = {
  name: string;
  key: string;
  unique: boolean;
};
type Store = {
  name: string;
  key: string | string[];
  auto: boolean;
  indexes: Index[];
};

const Store = (name: string, key: string | string[], auto: boolean, indexes: Index[]): Store => ({
  name,
  key,
  auto,
  indexes,
});
const Index = (name: string, key: string, unique: boolean): Index => ({ name, key, unique });
const stores: Store[] = [
  Store("status-widgets", ["owner", "id"], false, []),
  Store("layout", ["owner", "id"], false, []),
  Store("plugins", "package", false, []),
];

export function init() {
  return new Promise((resolve, reject) => {
    const request = indexedDB.open("pato-db", version);

    request.onerror = () => reject(request.error);
    request.onsuccess = () => resolve((db = request.result));
    request.onupgradeneeded = (event) => {
      // @ts-ignore
      const db: IDBDatabase = event?.target?.result;
      for (const def of stores) {
        if (!db.objectStoreNames.contains(def.name)) {
          const store = db.createObjectStore(def.name, {
            keyPath: def.key,
            autoIncrement: def.auto,
          });
          for (const idx of def.indexes) {
            store.createIndex(idx.name, idx.key, idx.unique ? { unique: true } : void 0);
          }
        }
      }
    };
  });
}

export async function set<T>(store: string, data: T): Promise<void> {
  return new Promise((resolve, reject) => {
    const tx = db.transaction(store, "readwrite");
    const request = tx.objectStore(store).put(data);
    request.onsuccess = () => resolve();
    request.onerror = () => reject(request.error);
  });
}

export async function get<T>(store: string, key: IDBValidKey | IDBKeyRange, index?: string): Promise<T | undefined> {
  return new Promise((resolve, reject) => {
    const tx = db.transaction(store, "readonly");
    const source = index ? tx.objectStore(store).index(index) : tx.objectStore(store);
    const request = source.get(key);
    request.onsuccess = () => resolve(request.result);
    request.onerror = () => reject(request.error);
  });
}

export async function getAll<T>(store: string): Promise<T[]> {
  return new Promise((resolve, reject) => {
    const tx = db.transaction(store, "readonly");
    const request = tx.objectStore(store).getAll();
    request.onsuccess = () => resolve(request.result);
    request.onerror = () => reject(request.error);
  });
}

export async function del(store: string, key: IDBValidKey | IDBKeyRange): Promise<void> {
  return new Promise((resolve, reject) => {
    const tx = db.transaction(store, "readwrite");
    const request = tx.objectStore(store).delete(key);
    request.onsuccess = () => resolve();
    request.onerror = () => reject(request.error);
  });
}

export async function count(store: string, query?: IDBValidKey | IDBKeyRange, index?: string): Promise<number> {
  return new Promise((resolve, reject) => {
    const tx = db.transaction(store, "readonly");
    const source = index ? tx.objectStore(store).index(index) : tx.objectStore(store);
    const request = source.count(query);
    request.onsuccess = () => resolve(request.result);
    request.onerror = () => reject(request.error);
  });
}
type Bound = { key: string; open: boolean };
type QueryRange = { lower: Bound } | { upper: Bound } | { lower: Bound; upper: Bound };
function createRange(range: QueryRange): IDBKeyRange {
  if ("lower" in range && "upper" in range) {
    return IDBKeyRange.bound(range.lower.key, range.upper.key, range.lower.open ?? false, range.upper.open ?? false);
  }
  if ("lower" in range) {
    return IDBKeyRange.lowerBound(range.lower.key, range.lower.open ?? false);
  }
  if ("upper" in range) {
    return IDBKeyRange.upperBound(range.upper.key, range.upper.open ?? false);
  }
  throw new Error("Invalid QueryRange");
}
type DbQuery = { key: string } | { keys: string[] } | { range: QueryRange };
function createQuery(query: DbQuery): IDBValidKey | IDBKeyRange {
  if ("key" in query) {
    return query.key;
  }
  if ("keys" in query) {
    return query.keys;
  }
  if ("range" in query) {
    return createRange(query.range);
  }
  throw new Error("Invalid Query");
}
type Entry = Record<string, string>;
function serializeResult(result: Entry | Entry[]): [string, string][][] {
  if (Array.isArray(result)) {
    return result.map((item) => Object.entries(item).map(([k, v]) => [k, v]));
  }
  return [Object.entries(result).map(([k, v]) => [k, v])];
}
type DbGet = {
  plugin: string;
  store: string;
  index?: string;
  query: DbQuery;
};
bind(async function dbGet({ plugin, store, index, query }: DbGet): Promise<[string, string][][]> {
  const result = await get<Entry | Entry[]>(`${plugin}|${store}`, createQuery(query), index);
  if (!result) return [];
  return serializeResult(result);
});
type DbSet = {
  plugin: string;
  store: string;
  data: [string, string][];
};
bind(async function dbSet({ plugin, store, data }: DbSet): Promise<boolean> {
  const entry = data.reduce((obj, [k, v]) => {
    obj[k] = v;
    return obj;
  }, {} as Entry);
  await set<Entry>(`${plugin}|${store}`, entry);
  return true;
});
type DbGetAll = {
  plugin: string;
  store: string;
};
bind(async function dbGetAll({ plugin, store }: DbGetAll): Promise<[string, string][][]> {
  const result = await getAll<Entry>(`${plugin}|${store}`);
  return serializeResult(result);
});
type DbDel = {
  plugin: string;
  store: string;
  query: DbQuery;
};
bind(async function dbDel({ plugin, store, query }: DbDel): Promise<boolean> {
  await del(`${plugin}|${store}`, createQuery(query));
  return true;
});
type DbCount = {
  plugin: string;
  store: string;
  index?: string;
  query?: DbQuery;
};
bind(async function dbCount({ plugin, store, index, query }: DbCount): Promise<number> {
  return await count(`${plugin}|${store}`, query ? createQuery(query) : void 0, index);
});
