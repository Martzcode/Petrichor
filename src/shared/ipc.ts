import { inject, Injectable, signal } from '@angular/core';
import type { MeasureSnapshot, RenderModel } from '../renderer/skin-model';

declare global {
  interface Window {
    __TAURI_INTERNALS__?: unknown;
  }
}

export function isTauri(): boolean {
  return typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window;
}

export type LogLevel = 'error' | 'warning' | 'notice' | 'debug';

export interface LogEntry {
  level: LogLevel;
  timestamp_ms: number;
  source: string | null;
  message: string;
}

export type DiagnosticSeverity = 'error' | 'warning' | 'info';

export interface Diagnostic {
  severity: DiagnosticSeverity;
  file: string | null;
  line: number | null;
  section: string | null;
  key: string | null;
  message: string;
}

export interface Diagnostics {
  items: Diagnostic[];
}

export interface WindowFlags {
  always_on_top: boolean;
  always_on_bottom?: boolean;
  on_hover?: number;
  click_through: boolean;
  draggable: boolean;
  skip_taskbar: boolean;
  opacity?: number;
  fade_duration?: number;
}

export interface SkinWindowInfo {
  label: string;
  path: string;
  name?: string;
  groups?: string[];
  always_on_top: boolean;
  click_through: boolean;
  draggable: boolean;
  skip_taskbar: boolean;
}

export type MouseActionState = 'enabled' | 'disabled' | 'cleared' | 'toggle';

export type SkinCommand =
  | { command: 'redraw' }
  | { command: 'update' }
  | { command: 'refresh' }
  | { command: 'draggable'; value: boolean }
  | {
      command: 'mouseActions';
      target: string;
      actions: string[];
      state: MouseActionState;
    }
  | { command: 'opacity'; value: number }
  | { command: 'fade'; show: boolean; duration_ms: number }
  | {
      command: 'selection';
      active: boolean;
      selected: boolean;
      color: string;
    }
  | { command: 'onHover'; value: number };

export interface AppPaths {
  config: string | null;
  skins: string | null;
  log: string | null;
}

export type SkinToggleState = 'created' | 'shown' | 'hidden';

export interface SkinToggleResult {
  label: string;
  state: SkinToggleState;
}

export interface RenderLoadResult {
  model: RenderModel;
  diagnostics: Diagnostics;
  flags: WindowFlags;
}

export interface SkinFile {
  filename: string;
  path: string;
  is_favorite: boolean;
}

export interface SkinFolder {
  name: string;
  path: string;
  level: number;
  files: SkinFile[];
  base_id: number;
  active_variant: number;
  has_favorite: boolean;
}

export interface Registry {
  root: string;
  folders: SkinFolder[];
}

export interface GlobalSettings {
  debug: boolean;
  logging: boolean;
  net_in_speed: number;
  net_out_speed: number;
  disable_dragging: boolean;
  disable_version_check: boolean;
  disable_auto_update: boolean;
  hardware_acceleration: number;
  skin_path: string;
  language: number | null;
  tray_icon: boolean;
  normal_stay_desktop: boolean;
}

export interface SystemInfo {
  version: string;
  os_name: string;
  os_version: string;
  hostname: string;
  user_name: string;
  uptime_secs: number;
}

@Injectable({ providedIn: 'root' })
export class PetrichorIpc {
  private readonly version = signal<string>('0.0.0');
  private readonly system = signal<SystemInfo | null>(null);
  private readonly logs = signal<LogEntry[]>([]);
  private readonly available = signal<boolean>(isTauri());

  readonly appVersion = this.version.asReadonly();
  readonly systemInfo = this.system.asReadonly();
  readonly logEntries = this.logs.asReadonly();
  readonly hasBackend = this.available.asReadonly();

  async refreshVersion(): Promise<void> {
    if (!this.available()) return;
    const { invoke } = await import('@tauri-apps/api/core');
    this.version.set(await invoke<string>('get_version'));
  }

  async refreshSystem(): Promise<void> {
    if (!this.available()) return;
    const { invoke } = await import('@tauri-apps/api/core');
    this.system.set(await invoke<SystemInfo>('get_system_info'));
  }

  async refreshLogs(): Promise<void> {
    if (!this.available()) return;
    const { invoke } = await import('@tauri-apps/api/core');
    this.logs.set(await invoke<LogEntry[]>('get_logs'));
  }

  async clearLogs(): Promise<void> {
    if (!this.available()) return;
    const { invoke } = await import('@tauri-apps/api/core');
    await invoke('clear_logs');
    await this.refreshLogs();
  }

  async emitDemoEvent(): Promise<void> {
    if (!this.available()) return;
    const { invoke } = await import('@tauri-apps/api/core');
    await invoke('demo_event');
    await this.refreshLogs();
  }

  async onLog(callback: (entry: LogEntry) => void): Promise<void> {
    if (!this.available()) return;
    const { listen } = await import('@tauri-apps/api/event');
    await listen<LogEntry>('petrichor://log', (event) =>
      callback(event.payload),
    );
  }

  async onCommand(
    callback: (command: SkinCommand) => void,
  ): Promise<() => void> {
    if (!this.available()) return () => undefined;
    const { listen } = await import('@tauri-apps/api/event');
    return listen<SkinCommand>('petrichor://command', (event) =>
      callback(event.payload),
    );
  }

  async currentWindowLabel(): Promise<string> {
    if (!this.available()) return 'main';
    const { getCurrentWindow } = await import('@tauri-apps/api/window');
    return getCurrentWindow().label;
  }

  async skinWindowInfo(label: string): Promise<SkinWindowInfo | null> {
    if (!this.available()) return null;
    const { invoke } = await import('@tauri-apps/api/core');
    return invoke<SkinWindowInfo | null>('skin_window_info', { label });
  }

  async loadRenderModel(path: string): Promise<RenderLoadResult | null> {
    if (!this.available()) return null;
    const { invoke } = await import('@tauri-apps/api/core');
    return invoke<RenderLoadResult>('load_render_model', { path });
  }

  async readMeasures(path: string): Promise<MeasureSnapshot> {
    if (!this.available()) return { strings: {}, values: {} };
    const { invoke } = await import('@tauri-apps/api/core');
    return invoke<MeasureSnapshot>('read_measures', { path });
  }

  async executeBang(label: string | null, command: string): Promise<void> {
    if (!this.available()) return;
    const { invoke } = await import('@tauri-apps/api/core');
    await invoke('execute_bang', { label, command });
  }

  async logBackend(level: LogLevel, message: string): Promise<void> {
    if (!this.available()) return;
    const { invoke } = await import('@tauri-apps/api/core');
    await invoke('frontend_log', { level, message });
  }

  async toggleSkinWindow(path: string): Promise<SkinToggleResult | null> {
    if (!this.available()) return null;
    const { invoke } = await import('@tauri-apps/api/core');
    return invoke<SkinToggleResult>('toggle_skin_window', { path });
  }

  async closeSkinWindow(label: string): Promise<void> {
    if (!this.available()) return;
    const { invoke } = await import('@tauri-apps/api/core');
    await invoke('close_skin_window', { label });
  }

  async listSkinWindows(): Promise<SkinWindowInfo[]> {
    if (!this.available()) return [];
    const { invoke } = await import('@tauri-apps/api/core');
    return invoke<SkinWindowInfo[]>('list_skin_windows');
  }

  async applySkinWindowFlags(label: string, flags: WindowFlags): Promise<void> {
    if (!this.available()) return;
    const { invoke } = await import('@tauri-apps/api/core');
    await invoke('apply_skin_window_flags', { label, flags });
  }

  async fitSkinWindow(
    label: string,
    width: number,
    height: number,
  ): Promise<void> {
    if (!this.available()) return;
    const { invoke } = await import('@tauri-apps/api/core');
    await invoke('fit_skin_window', { label, width, height });
  }

  async showSkinWindow(label: string): Promise<void> {
    if (!this.available()) return;
    const { invoke } = await import('@tauri-apps/api/core');
    await invoke('show_skin_window', { label });
  }

  async showSkinMenu(label: string, x?: number, y?: number): Promise<void> {
    if (!this.available()) return;
    const { invoke } = await import('@tauri-apps/api/core');
    await invoke('show_skin_menu', { label, x, y });
  }

  async hideSkinWindow(label: string): Promise<void> {
    if (!this.available()) return;
    const { invoke } = await import('@tauri-apps/api/core');
    await invoke('hide_skin_window', { label });
  }

  async saveSkinPosition(label: string): Promise<void> {
    if (!this.available()) return;
    const { invoke } = await import('@tauri-apps/api/core');
    await invoke('save_skin_position', { label });
  }

  async setSkinDragging(label: string, dragging: boolean): Promise<void> {
    if (!this.available()) return;
    const { invoke } = await import('@tauri-apps/api/core');
    await invoke('set_skin_dragging', { label, dragging });
  }

  async selectSkin(label: string, additive: boolean): Promise<boolean> {
    if (!this.available()) return false;
    const { invoke } = await import('@tauri-apps/api/core');
    return invoke<boolean>('select_skin', { label, additive });
  }

  async selectionActive(): Promise<boolean> {
    if (!this.available()) return false;
    const { invoke } = await import('@tauri-apps/api/core');
    return invoke<boolean>('selection_active');
  }

  async assetUrl(path: string): Promise<string> {
    if (!this.available()) return path;
    const { convertFileSrc } = await import('@tauri-apps/api/core');
    return convertFileSrc(path);
  }

  async defaultSkinRoot(): Promise<string | null> {
    if (!this.available()) return null;
    const { invoke } = await import('@tauri-apps/api/core');
    return invoke<string | null>('default_skin_root');
  }

  async getPaths(): Promise<AppPaths | null> {
    if (!this.available()) return null;
    const { invoke } = await import('@tauri-apps/api/core');
    return invoke<AppPaths>('get_paths');
  }

  async scanSkins(root: string): Promise<Registry> {
    const { invoke } = await import('@tauri-apps/api/core');
    return invoke<Registry>('scan_skins', { root });
  }
}

export const ipc = {
  inject: () => inject(PetrichorIpc),
};
