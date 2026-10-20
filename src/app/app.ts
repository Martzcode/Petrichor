import {
  ChangeDetectionStrategy,
  Component,
  OnInit,
  inject,
  signal,
} from '@angular/core';
import {
  AppPaths,
  LogEntry,
  PetrichorIpc,
  SkinFolder,
  SkinWindowInfo,
} from '../shared/ipc';

@Component({
  selector: 'app-root',
  templateUrl: './app.html',
  styleUrl: './app.css',
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class App implements OnInit {
  protected readonly ipc = inject(PetrichorIpc);
  protected readonly title = signal('Petrichor');
  protected readonly logEntries = signal<LogEntry[]>([]);
  protected readonly skins = signal<SkinFolder[]>([]);
  protected readonly notice = signal<string | null>(null);
  protected readonly aboutOpen = signal(false);
  protected readonly paths = signal<AppPaths | null>(null);
  protected readonly windows = signal<SkinWindowInfo[]>([]);

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

    await this.refreshSkins();
    await this.refreshWindows();
  }

  protected async openAbout(): Promise<void> {
    this.paths.set(await this.ipc.getPaths());
    this.aboutOpen.set(true);
  }

  protected closeAbout(): void {
    this.aboutOpen.set(false);
  }

  protected async clearLogs(): Promise<void> {
    await this.ipc.clearLogs();
    this.logEntries.set(this.ipc.logEntries());
  }

  protected async emitDemoEvent(): Promise<void> {
    await this.ipc.emitDemoEvent();
    this.logEntries.set(this.ipc.logEntries());
  }

  protected async toggleSkin(path: string): Promise<void> {
    this.notice.set(null);
    try {
      const result = await this.ipc.toggleSkinWindow(path);
      await this.ipc.refreshLogs();
      this.logEntries.set(this.ipc.logEntries());
      if (!result) {
        this.notice.set('Backend Petrichor indisponible.');
        return;
      }
      await this.refreshWindows();
      const name = path.split('/').pop() ?? path;
      this.notice.set(
        result.state === 'hidden'
          ? `Skin « ${name} » masqué`
          : `Skin « ${name} » affiché`,
      );
    } catch (cause) {
      const message = cause instanceof Error ? cause.message : String(cause);
      this.notice.set(`Échec de la bascule du skin : ${message}`);
      await this.ipc.logBackend(
        'error',
        `Bascule du skin échouée : ${message}`,
      );
      await this.ipc.refreshLogs();
      this.logEntries.set(this.ipc.logEntries());
    }
  }

  protected labelFor(path: string): string | null {
    return this.windows().find((window) => window.path === path)?.label ?? null;
  }

  protected async closeSkin(label: string): Promise<void> {
    try {
      await this.ipc.closeSkinWindow(label);
    } catch (cause) {
      const message = cause instanceof Error ? cause.message : String(cause);
      this.notice.set(`Échec de la fermeture du skin : ${message}`);
    }
    await this.refreshWindows();
    await this.ipc.refreshLogs();
    this.logEntries.set(this.ipc.logEntries());
  }

  private async refreshWindows(): Promise<void> {
    if (!this.ipc.hasBackend()) {
      return;
    }
    this.windows.set(await this.ipc.listSkinWindows());
  }

  private async refreshSkins(): Promise<void> {
    const root = await this.ipc.defaultSkinRoot();
    if (!root) return;
    const registry = await this.ipc.scanSkins(root);
    this.skins.set(registry.folders);
  }
}
