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
  selected_color: { r: number; g: number; b: number; a: number } | null;
  skin_editor: string | null;
  tray_execute_r: string | null;
  tray_execute_m: string | null;
  tray_execute_dr: string | null;
  tray_execute_dm: string | null;
  disable_version_check: boolean;
  disable_auto_update: boolean;
  update_url: string | null;
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

  async manageSnapshot(): Promise<ManageSnapshot | null> {
    if (!this.available()) return null;
    const { invoke } = await import('@tauri-apps/api/core');
    return invoke<ManageSnapshot>('manage_snapshot');
  }

  async skinDetails(path: string): Promise<SkinDetails | null> {
    if (!this.available()) return null;
    const { invoke } = await import('@tauri-apps/api/core');
    return invoke<SkinDetails>('skin_details', { path });
  }

  async updateSkinState(
    path: string,
    patch: SkinStatePatch,
  ): Promise<SkinState | null> {
    if (!this.available()) return null;
    const { invoke } = await import('@tauri-apps/api/core');
    return invoke<SkinState>('update_skin_state', { path, patch });
  }

  async setActiveVariant(path: string): Promise<void> {
    if (!this.available()) return;
    const { invoke } = await import('@tauri-apps/api/core');
    await invoke('set_active_variant', { path });
  }

  async setSkinFavorite(path: string, favorite: boolean): Promise<void> {
    if (!this.available()) return;
    const { invoke } = await import('@tauri-apps/api/core');
    await invoke('set_skin_favorite', { path, favorite });
  }

  async setLayoutFavorite(name: string, favorite: boolean): Promise<void> {
    if (!this.available()) return;
    const { invoke } = await import('@tauri-apps/api/core');
    await invoke('set_layout_favorite', { name, favorite });
  }

  async getSettings(): Promise<GlobalSettings | null> {
    if (!this.available()) return null;
    const { invoke } = await import('@tauri-apps/api/core');
    return invoke<GlobalSettings>('settings');
  }

  async saveSettings(settings: GlobalSettings): Promise<void> {
    if (!this.available()) return;
    const { invoke } = await import('@tauri-apps/api/core');
    await invoke('save_settings', { settings });
  }

  async saveLayout(
    name: string,
    excludeUnused: boolean,
    includeWallpaper: boolean,
  ): Promise<LayoutInfo | null> {
    if (!this.available()) return null;
    const { invoke } = await import('@tauri-apps/api/core');
    return invoke<LayoutInfo>('save_layout', {
      name,
      excludeUnused,
      includeWallpaper,
    });
  }

  async deleteLayout(name: string): Promise<void> {
    if (!this.available()) return;
    const { invoke } = await import('@tauri-apps/api/core');
    await invoke('delete_layout', { name });
  }

  async editLayout(name: string): Promise<void> {
    if (!this.available()) return;
    const { invoke } = await import('@tauri-apps/api/core');
    await invoke('edit_layout', { name });
  }

  async loadLayout(name: string): Promise<number> {
    if (!this.available()) return 0;
    const { invoke } = await import('@tauri-apps/api/core');
    return invoke<number>('load_layout', { name });
  }

  async saveGameMode(config: GameModeConfig): Promise<void> {
    if (!this.available()) return;
    const { invoke } = await import('@tauri-apps/api/core');
    await invoke('set_game_mode_config', { config });
  }

  async toggleGameMode(): Promise<boolean> {
    if (!this.available()) return false;
    const { invoke } = await import('@tauri-apps/api/core');
    return invoke<boolean>('toggle_game_mode');
  }

  async resetStatistics(): Promise<number> {
    if (!this.available()) return 0;
    const { invoke } = await import('@tauri-apps/api/core');
    return invoke<number>('reset_statistics');
  }

  async deleteLogFile(): Promise<void> {
    if (!this.available()) return;
    const { invoke } = await import('@tauri-apps/api/core');
    await invoke('delete_log_file');
  }

  async openLogFolder(): Promise<void> {
    if (!this.available()) return;
    const { invoke } = await import('@tauri-apps/api/core');
    await invoke('open_log_folder');
  }

  async openSkinFolder(path: string): Promise<void> {
    if (!this.available()) return;
    const { invoke } = await import('@tauri-apps/api/core');
    await invoke('open_skin_folder', { path });
  }

  async openSkinEditor(path: string): Promise<void> {
    if (!this.available()) return;
    const { invoke } = await import('@tauri-apps/api/core');
    await invoke('open_skin_editor', { path });
  }

  async createSkin(spec: NewSkinSpec): Promise<NewSkinResult | null> {
    if (!this.available()) return null;
    const { invoke } = await import('@tauri-apps/api/core');
    return invoke<NewSkinResult>('create_skin', { spec });
  }

  async onPackageProgress(
    callback: (progress: PackageProgress) => void,
  ): Promise<() => void> {
    if (!this.available()) return () => undefined;
    const { listen } = await import('@tauri-apps/api/event');
    return listen<PackageProgress>('petrichor://package-progress', (event) =>
      callback(event.payload),
    );
  }

  async inspectPackage(path: string): Promise<PackageInfo | null> {
    if (!this.available()) return null;
    const { invoke } = await import('@tauri-apps/api/core');
    return invoke<PackageInfo>('inspect_package', { path });
  }

  async createPackage(
    spec: CreatePackageSpec,
  ): Promise<CreatePackageResult | null> {
    if (!this.available()) return null;
    const { invoke } = await import('@tauri-apps/api/core');
    return invoke<CreatePackageResult>('create_package', { spec });
  }

  async installPackage(
    path: string,
    components: string[],
    merge: boolean,
    preserveVariables: boolean,
  ): Promise<InstallResult | null> {
    if (!this.available()) return null;
    const { invoke } = await import('@tauri-apps/api/core');
    return invoke<InstallResult>('install_package', {
      path,
      components,
      merge,
      preserveVariables,
    });
  }

  async checkUpdates(endpoint?: string): Promise<UpdateStatus | null> {
    if (!this.available()) return null;
    const { invoke } = await import('@tauri-apps/api/core');
    return invoke<UpdateStatus>('check_updates', { endpoint });
  }

  async downloadUpdate(
    url: string,
    sha256: string,
    filename?: string,
    version?: string,
  ): Promise<string> {
    if (!this.available()) return '';
    const { invoke } = await import('@tauri-apps/api/core');
    return invoke<string>('download_update', {
      url,
      sha256,
      filename,
      version,
    });
  }

  async installUpdate(path?: string): Promise<string> {
    if (!this.available()) return '';
    const { invoke } = await import('@tauri-apps/api/core');
    return invoke<string>('install_update', { path });
  }
}

export const ipc = {
  inject: () => inject(PetrichorIpc),
};

export interface LayoutInfo {
  name: string;
  path: string;
  modified_ms: number;
  wallpaper: boolean;
  skin_count: number;
  is_favorite: boolean;
}

export interface SkinState {
  window_x: number;
  window_y: number;
  anchor_x: number;
  anchor_y: number;
  always_on_top: number;
  on_hover: number;
  draggable: boolean;
  snap_edges: boolean;
  click_through: boolean;
  keep_on_screen: boolean;
  save_position: boolean;
  start_hidden: boolean;
  auto_select_screen: boolean;
  alpha_value: number;
  fade_duration: number;
  group: string[];
  drag_group: string[];
  active: number;
  load_order: number | null;
  defaults: Record<string, string>;
}

export interface SkinMetadata {
  name: string | null;
  author: string | null;
  information: string | null;
  version: string | null;
  license: string | null;
}

export interface SkinDetails {
  path: string;
  name: string;
  folder: string;
  metadata: SkinMetadata;
  state: SkinState;
  favorites: boolean;
  open_label: string | null;
}

export interface SkinStatePatch {
  always_on_top?: number;
  click_through?: boolean;
  draggable?: boolean;
  keep_on_screen?: boolean;
  snap_edges?: boolean;
  save_position?: boolean;
  auto_select_screen?: boolean;
  on_hover?: number;
  alpha_value?: number;
  fade_duration?: number;
  active?: number;
  load_order?: number;
  window_x?: number;
  window_y?: number;
  anchor_x?: number;
  anchor_y?: number;
  start_hidden?: boolean;
  monitor?: number;
}

export interface GameModeConfig {
  start_action: string;
  stop_action: string;
  detect_fullscreen: boolean;
  detect_process: boolean;
  process_list: string[];
}

export interface GameModeStatus {
  active: boolean;
  config: GameModeConfig;
  fullscreen: boolean;
  process: boolean;
}

export interface MonitorInfo {
  index: number;
  name: string;
  x: number;
  y: number;
  width: number;
  height: number;
  primary: boolean;
}

export interface ManageSnapshot {
  version: string;
  skins: Registry;
  windows: SkinWindowInfo[];
  layouts: LayoutInfo[];
  settings: GlobalSettings;
  game_mode: GameModeStatus;
  paths: AppPaths;
  monitors: MonitorInfo[];
}

export interface NewSkinSpec {
  name: string;
  author: string;
  width: number;
  height: number;
}

export interface NewSkinResult {
  path: string;
  folder: string;
}

export interface PackageMetadata {
  name: string;
  author: string;
  version: string;
  load: string;
  merge_skins: boolean;
  variable_files: string[];
  minimum_petrichor: string;
  banner: string;
}

export interface PackageInfo {
  path: string;
  metadata: PackageMetadata;
  components: Record<string, number>;
  files: number;
  bytes: number;
}

export interface CreatePackageSpec {
  source: string;
  output: string;
  name: string;
  author: string;
  version: string;
  load: string;
  layouts: string[];
  include_plugins: boolean;
  merge_skins: boolean;
  variable_files: string[];
  banner: string | null;
  include_fonts: boolean;
  include_addons: boolean;
}

export interface PackageProgress {
  phase: string;
  current: number;
  total: number;
  message: string;
}

export interface CreatePackageResult {
  path: string;
  files: number;
  bytes: number;
}

export interface InstallResult {
  metadata: PackageMetadata;
  installed: string[];
  files: number;
  layout_loaded: string | null;
  warnings: string[];
}

export interface UpdateStatus {
  current: string;
  latest: string;
  available: boolean;
  url: string;
  sha256: string;
  notes: string;
  pending: boolean;
  pending_path: string | null;
}

export interface SkinOptionsSnapshot {
  disable_dragging: boolean;
}
