import {
  ChangeDetectionStrategy,
  Component,
  OnInit,
  inject,
  signal,
} from '@angular/core';
import { LogEntry, PetrichorIpc, SkinFolder } from '../shared/ipc';

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

  private async refreshSkins(): Promise<void> {
    const root = await this.ipc.defaultSkinRoot();
    if (!root) return;
    const registry = await this.ipc.scanSkins(root);
    this.skins.set(registry.folders);
  }
}
