import type { Component } from "svelte";
import ArrowCircleUp from "phosphor-svelte/lib/ArrowCircleUp";
import Car from "phosphor-svelte/lib/Car";
import Clock from "phosphor-svelte/lib/Clock";
import FileCode from "phosphor-svelte/lib/FileCode";
import MapTrifold from "phosphor-svelte/lib/MapTrifold";
import Package from "phosphor-svelte/lib/Package";
import Power from "phosphor-svelte/lib/Power";
import PuzzlePiece from "phosphor-svelte/lib/PuzzlePiece";
import Question from "phosphor-svelte/lib/Question";
import SpeakerHigh from "phosphor-svelte/lib/SpeakerHigh";
import Trash from "phosphor-svelte/lib/Trash";
import WarningCircle from "phosphor-svelte/lib/WarningCircle";
import type { StatusTone } from "./status.js";

/**
 * Иконки меток: тип мода читается быстрее значка, чем слова, а состояние — тем
 * более. Одна таблица на карточку, строку списка, панель мода и поиск.
 */

// Иконки Phosphor — компоненты с одинаковыми свойствами (size, weight).
type IconComponent = Component<{ size?: number; weight?: string }>;

const KIND: Record<string, IconComponent> = {
  vehicle: Car as unknown as IconComponent,
  map: MapTrifold as unknown as IconComponent,
  level: MapTrifold as unknown as IconComponent,
  script: FileCode as unknown as IconComponent,
  sound: SpeakerHigh as unknown as IconComponent,
  addon: PuzzlePiece as unknown as IconComponent,
};

/** Иконка типа; неизвестный тип — коробка «мод». */
export function kindIcon(tag: string | null | undefined): IconComponent {
  return (tag && KIND[tag]) || (Package as unknown as IconComponent);
}

const STATUS: Record<StatusTone, IconComponent> = {
  update: ArrowCircleUp as unknown as IconComponent,
  possible: Question as unknown as IconComponent,
  problem: WarningCircle as unknown as IconComponent,
  pending: Clock as unknown as IconComponent,
  off: Power as unknown as IconComponent,
  deleted: Trash as unknown as IconComponent,
};

export function statusIcon(tone: StatusTone): IconComponent {
  return STATUS[tone];
}

export const STATUS_TONES = Object.keys(STATUS) as StatusTone[];
