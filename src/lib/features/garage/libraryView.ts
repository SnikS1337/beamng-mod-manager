/** Что показывать в гараже: скелет, полный экран ошибки или уже загруженные моды. */
export type LibraryView = "skeleton" | "error" | "content";

export interface LibraryViewInput {
  loading: boolean;
  error: string | null;
  hasSnapshot: boolean;
}

export function libraryView({
  loading,
  error,
  hasSnapshot,
}: LibraryViewInput): LibraryView {
  if (loading && !hasSnapshot) return "skeleton";
  // Ошибка фоновой перезагрузки не должна стирать уже показанную библиотеку.
  if (error !== null && !hasSnapshot) return "error";
  return "content";
}

/** Полный экран ошибки заменяется баннером, пока есть что показывать. */
export function staleError({ error, hasSnapshot }: LibraryViewInput): boolean {
  return error !== null && hasSnapshot;
}
