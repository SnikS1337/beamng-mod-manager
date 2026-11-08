<script lang="ts">
  import { onMount } from "svelte";
  import { isTauri } from "@tauri-apps/api/core";
  import { getCurrentWindow } from "@tauri-apps/api/window";
  import { liveDownloads } from "#lib/features/download/live.svelte.js";
  import { overallProgress } from "#lib/features/download/jobs.js";
  import { updatesRun } from "#lib/features/updates/run.svelte.js";
  import { sameTaskbar, taskbarState, toWindowProgress, type TaskbarState } from "./taskbar.js";

  // Кнопка программы на панели задач: ход загрузок и проверки (см. taskbar.ts).
  // Ход фоновой проверки приходит, только если кто-то слушает события.
  onMount(() => updatesRun.start());

  let shown: TaskbarState | null = null;
  const wanted = $derived(
    taskbarState(
      overallProgress(liveDownloads.jobs, liveDownloads.tracked, liveDownloads.now),
      updatesRun.checking || updatesRun.autoProgress !== null,
    ),
  );

  $effect(() => {
    const next = wanted;
    if (!isTauri() || sameTaskbar(shown, next)) return;
    shown = next;
    void getCurrentWindow()
      .setProgressBar(toWindowProgress(next))
      .catch(() => {});
  });
</script>
