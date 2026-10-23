import {
  ChangeDetectionStrategy,
  Component,
  OnInit,
  inject,
  signal,
} from '@angular/core';
import { FormsModule } from '@angular/forms';
import {
  AppPaths,
  CreatePackageSpec,
  GameModeConfig,
  GlobalSettings,
  InstallResult,
  LayoutInfo,
  LogEntry,
  ManageSnapshot,
  NewSkinSpec,
  PackageInfo,
  PetrichorIpc,
  SkinDetails,
  SkinFolder,
  SkinStatePatch,
  SkinWindowInfo,
  UpdateStatus,
} from '../shared/ipc';

type Tab = 'skins' | 'layouts' | 'game' | 'settings' | 'packages';

@Component({
  selector: 'app-root',
  templateUrl: './app.html',
  styleUrl: './app.css',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [FormsModule],
})
export class App implements OnInit {
  protected readonly ipc = inject(PetrichorIpc);
  protected readonly title = signal('Petrichor');
  protected readonly tab = signal<Tab>('skins');
  protected readonly logEntries = signal<LogEntry[]>([]);
  protected readonly snapshot = signal<ManageSnapshot | null>(null);
  protected readonly details = signal<SkinDetails | null>(null);
  protected readonly notice = signal<string | null>(null);
  protected readonly error = signal<string | null>(null);
  protected readonly aboutOpen = signal(false);
  protected readonly busy = signal(false);
  protected readonly paths = signal<AppPaths | null>(null);
  protected readonly windows = signal<SkinWindowInfo[]>([]);
  protected readonly layoutName = signal('');
  protected readonly excludeUnused = signal(true);
  protected readonly includeWallpaper = signal(false);
  protected readonly showNewSkin = signal(false);
  protected readonly packageInfo = signal<PackageInfo | null>(null);
  protected readonly packagePath = signal('');
  protected readonly selectedComponents = signal<string[]>([]);
  protected readonly packageMerge = signal(true);
  protected readonly preserveVariables = signal(true);
  protected readonly installResult = signal<InstallResult | null>(null);
  protected readonly updateStatus = signal<UpdateStatus | null>(null);
  protected readonly downloadedUpdate = signal<string | null>(null);
  protected readonly search = signal('');

  protected settings: GlobalSettings | null = null;
  protected gameForm: GameModeConfig = emptyGameMode();
  protected processText = '';
  protected newSkinForm: NewSkinSpec = {
    name: 'Nouveau skin',
    author: '',
    width: 240,
    height: 100,
  };
  protected packageForm: CreatePackageSpec = emptyPackageSpec();

  async ngOnInit(): Promise<void> {
    if (!this.ipc.hasBackend()) {
      return;
    }

    await this.ipc.refreshVersion();
    await this.ipc.refreshSystem();
    await this.ipc.refreshLogs();
    this.logEntries.set(this.ipc.logEntries());

    await this.ipc.onLog((entry) => {
      this.logEntries.update((entries) => [...entries, entry]);
    });

    await this.run('Chargement de la gestion', async () => {
      await this.refreshSnapshot();
    });
  }

  protected setTab(tab: Tab): void {
    this.tab.set(tab);
    this.error.set(null);
  }

  protected async refreshSnapshot(): Promise<void> {
    const snapshot = await this.ipc.manageSnapshot();
    if (!snapshot) return;
    this.snapshot.set(snapshot);
    this.windows.set(snapshot.windows);
    this.paths.set(snapshot.paths);
    this.settings = {
      ...snapshot.settings,
      selected_color: snapshot.settings.selected_color
        ? { ...snapshot.settings.selected_color }
        : null,
    };
    this.gameForm = {
      ...snapshot.game_mode.config,
      process_list: [...snapshot.game_mode.config.process_list],
    };
    this.processText = this.gameForm.process_list.join('\n');
    if (!this.packageForm.source) {
      const folder = snapshot.skins.folders.find(
        (item) => item.files.length > 0,
      );
      this.packageForm.source = folder?.path ?? '';
      this.packageForm.name = folder?.name ?? 'Mon paquet';
      this.packageForm.output = folder ? `${folder.name}.zip` : 'petrichor.zip';
    }
  }

  protected async selectSkin(path: string): Promise<void> {
    await this.run('Chargement du skin', async () => {
      this.details.set(await this.ipc.skinDetails(path));
    });
  }

  protected async toggleSkin(path: string): Promise<void> {
    await this.run('Bascule du skin', async () => {
      const result = await this.ipc.toggleSkinWindow(path);
      if (!result) throw new Error('Backend Petrichor indisponible.');
      await this.refreshSnapshot();
      if (this.details()?.path === path) {
        this.details.set(await this.ipc.skinDetails(path));
      }
      const name = path.split('/').pop() ?? path;
      this.notice.set(
        result.state === 'hidden'
          ? `Skin « ${name} » masqué`
          : `Skin « ${name} » affiché`,
      );
    });
  }

  protected async closeSkin(label: string): Promise<void> {
    await this.run('Fermeture du skin', async () => {
      await this.ipc.closeSkinWindow(label);
      await this.refreshSnapshot();
    });
  }

  protected async patchSkin(patch: SkinStatePatch): Promise<void> {
    const details = this.details();
    if (!details) return;
    await this.run('Mise à jour du skin', async () => {
      await this.ipc.updateSkinState(details.path, patch);
      this.details.set(await this.ipc.skinDetails(details.path));
      await this.refreshSnapshot();
      this.details.set(await this.ipc.skinDetails(details.path));
    });
  }

  protected async toggleSkinFavorite(
    path: string,
    favorite: boolean,
  ): Promise<void> {
    await this.run('Favori', async () => {
      await this.ipc.setSkinFavorite(path, !favorite);
      await this.refreshSnapshot();
      if (this.details()?.path === path) {
        this.details.set(await this.ipc.skinDetails(path));
      }
    });
  }

  protected async toggleLayoutFavorite(layout: LayoutInfo): Promise<void> {
    await this.run('Favori de layout', async () => {
      await this.ipc.setLayoutFavorite(layout.name, !layout.is_favorite);
      await this.refreshSnapshot();
    });
  }

  protected async saveLayout(): Promise<void> {
    const name = this.layoutName().trim();
    if (!name) {
      this.error.set('Indiquez un nom de layout.');
      return;
    }
    await this.run('Enregistrement du layout', async () => {
      await this.ipc.saveLayout(
        name,
        this.excludeUnused(),
        this.includeWallpaper(),
      );
      this.layoutName.set('');
      await this.refreshSnapshot();
      this.notice.set(`Layout « ${name} » enregistré`);
    });
  }

  protected async loadLayout(name: string): Promise<void> {
    await this.run('Chargement du layout', async () => {
      const count = await this.ipc.loadLayout(name);
      await this.refreshSnapshot();
      this.notice.set(`Layout « ${name} » chargé (${count} skins)`);
    });
  }

  protected async deleteLayout(name: string): Promise<void> {
    if (!window.confirm(`Supprimer le layout « ${name} » ?`)) return;
    await this.run('Suppression du layout', async () => {
      await this.ipc.deleteLayout(name);
      await this.refreshSnapshot();
    });
  }

  protected async editLayout(name: string): Promise<void> {
    await this.run('Édition du layout', () => this.ipc.editLayout(name));
  }

  protected async saveGameMode(): Promise<void> {
    const config: GameModeConfig = {
      ...this.gameForm,
      process_list: this.processText
        .split('\n')
        .map((line) => line.trim())
        .filter((line) => line.length > 0),
    };
    await this.run('Enregistrement du Game Mode', async () => {
      await this.ipc.saveGameMode(config);
      await this.refreshSnapshot();
      this.notice.set('Game Mode enregistré');
    });
  }

  protected async toggleGameMode(): Promise<void> {
    await this.run('Bascule du Game Mode', async () => {
      const active = await this.ipc.toggleGameMode();
      await this.refreshSnapshot();
      this.notice.set(active ? 'Game Mode activé' : 'Game Mode désactivé');
    });
  }

  protected async saveGlobalSettings(): Promise<void> {
    if (!this.settings) return;
    await this.run('Enregistrement des réglages', async () => {
      await this.ipc.saveSettings(this.settings!);
      await this.refreshSnapshot();
      this.notice.set('Réglages enregistrés');
    });
  }

  protected async checkUpdates(): Promise<void> {
    await this.run('Vérification des mises à jour', async () => {
      this.updateStatus.set(await this.ipc.checkUpdates());
    });
  }

  protected async downloadUpdate(): Promise<void> {
    const status = this.updateStatus();
    if (!status?.url) {
      this.error.set('Aucune mise à jour téléchargeable.');
      return;
    }
    await this.run('Téléchargement de la mise à jour', async () => {
      const path = await this.ipc.downloadUpdate(
        status.url,
        status.sha256,
        `${status.latest}.package`,
      );
      this.downloadedUpdate.set(path);
      this.notice.set(`Mise à jour téléchargée : ${path}`);
    });
  }

  protected async resetStatistics(): Promise<void> {
    await this.run('Réinitialisation des statistiques', async () => {
      const removed = await this.ipc.resetStatistics();
      this.notice.set(`${removed} fichier(s) de statistiques supprimé(s)`);
    });
  }

  protected async deleteLog(): Promise<void> {
    if (!window.confirm('Supprimer le journal ?')) return;
    await this.run('Suppression du journal', async () => {
      await this.ipc.deleteLogFile();
      this.logEntries.set([]);
      this.notice.set('Journal supprimé');
    });
  }

  protected async openLogFolder(): Promise<void> {
    await this.run('Ouverture du journal', () => this.ipc.openLogFolder());
  }

  protected async openSkinFolder(path: string): Promise<void> {
    await this.run('Ouverture du dossier', () => this.ipc.openSkinFolder(path));
  }

  protected async openSkinEditor(path: string): Promise<void> {
    await this.run('Ouverture de l’éditeur', () =>
      this.ipc.openSkinEditor(path),
    );
  }

  protected async createNewSkin(): Promise<void> {
    await this.run('Création du skin', async () => {
      const result = await this.ipc.createSkin(this.newSkinForm);
      if (!result) throw new Error('Création impossible.');
      this.showNewSkin.set(false);
      await this.refreshSnapshot();
      await this.selectSkin(result.path);
      this.notice.set(`Skin créé : ${result.folder}`);
    });
  }

  protected async inspectPackage(): Promise<void> {
    await this.run('Inspection du paquet', async () => {
      const info = await this.ipc.inspectPackage(this.packagePath());
      if (!info) throw new Error('Paquet illisible.');
      this.packageInfo.set(info);
      this.selectedComponents.set(Object.keys(info.components));
      this.installResult.set(null);
    });
  }

  protected toggleComponent(component: string): void {
    this.selectedComponents.update((values) =>
      values.includes(component)
        ? values.filter((value) => value !== component)
        : [...values, component],
    );
  }

  protected async installPackage(): Promise<void> {
    await this.run('Installation du paquet', async () => {
      const result = await this.ipc.installPackage(
        this.packagePath(),
        this.selectedComponents(),
        this.packageMerge(),
        this.preserveVariables(),
      );
      this.installResult.set(result);
      await this.refreshSnapshot();
      this.notice.set(
        result
          ? `Paquet installé (${result.files} fichiers)`
          : 'Paquet installé',
      );
    });
  }

  protected async createPackage(): Promise<void> {
    await this.run('Création du paquet', async () => {
      const result = await this.ipc.createPackage(this.packageForm);
      if (!result) throw new Error('Création impossible.');
      this.packagePath.set(result.path);
      this.notice.set(
        `Paquet créé : ${result.path} (${result.files} fichiers)`,
      );
      await this.inspectPackage();
    });
  }

  protected choosePackageSource(path: string): void {
    this.packageForm.source = path;
    const folder = this.snapshot()?.skins.folders.find(
      (item) => item.path === path,
    );
    if (folder) {
      this.packageForm.name = folder.name;
      this.packageForm.output = `${folder.name}.zip`;
    }
  }

  protected togglePackageLayout(name: string): void {
    this.packageForm.layouts = this.packageForm.layouts.includes(name)
      ? this.packageForm.layouts.filter((value) => value !== name)
      : [...this.packageForm.layouts, name];
  }

  protected hasPackageLayout(name: string): boolean {
    return this.packageForm.layouts.includes(name);
  }

  protected componentEntries(): Array<[string, number]> {
    const info = this.packageInfo();
    return info ? Object.entries(info.components) : [];
  }

  protected visibleFolders(): SkinFolder[] {
    const term = this.search().trim().toLowerCase();
    const folders = this.snapshot()?.skins.folders ?? [];
    if (!term) return folders;
    return folders.filter(
      (folder) =>
        folder.name.toLowerCase().includes(term) ||
        folder.files.some((file) => file.filename.toLowerCase().includes(term)),
    );
  }

  protected checked(event: Event): boolean {
    return (event.target as HTMLInputElement).checked;
  }

  protected inputValue(event: Event): string {
    return (event.target as HTMLInputElement | HTMLSelectElement).value;
  }

  protected numberValue(event: Event): number {
    return Number((event.target as HTMLInputElement).value);
  }

  protected formatBytes(bytes: number): string {
    if (bytes < 1024) return `${bytes} o`;
    if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} Kio`;
    return `${(bytes / 1024 / 1024).toFixed(1)} Mio`;
  }

  protected formatDate(timestamp: number): string {
    if (!timestamp) return '—';
    return new Date(timestamp).toLocaleString();
  }

  protected openAbout(): void {
    this.paths.set(this.snapshot()?.paths ?? null);
    this.aboutOpen.set(true);
  }

  protected closeAbout(): void {
    this.aboutOpen.set(false);
  }

  protected async clearLogs(): Promise<void> {
    await this.run('Effacement du journal', async () => {
      await this.ipc.clearLogs();
      this.logEntries.set(this.ipc.logEntries());
    });
  }

  protected async emitDemoEvent(): Promise<void> {
    await this.run('Événement de démonstration', async () => {
      await this.ipc.emitDemoEvent();
      this.logEntries.set(this.ipc.logEntries());
    });
  }

  protected labelFor(path: string): string | null {
    return this.windows().find((window) => window.path === path)?.label ?? null;
  }

  private async run(
    label: string,
    action: () => Promise<unknown>,
  ): Promise<void> {
    this.busy.set(true);
    this.error.set(null);
    this.notice.set(null);
    try {
      await action();
    } catch (cause) {
      const message = cause instanceof Error ? cause.message : String(cause);
      this.error.set(`${label} : ${message}`);
      await this.ipc.logBackend('error', `${label} : ${message}`);
    } finally {
      this.busy.set(false);
    }
  }
}

function emptyGameMode(): GameModeConfig {
  return {
    start_action: '',
    stop_action: '',
    detect_fullscreen: false,
    detect_process: false,
    process_list: [],
  };
}

function emptyPackageSpec(): CreatePackageSpec {
  return {
    source: '',
    output: 'petrichor.zip',
    name: '',
    author: '',
    version: '1.0',
    load: '',
    layouts: [],
    include_plugins: false,
    merge_skins: true,
  };
}
