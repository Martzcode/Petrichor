import { inject, Injectable, signal } from '@angular/core';

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
}

export const ipc = {
  inject: () => inject(PetrichorIpc),
};
