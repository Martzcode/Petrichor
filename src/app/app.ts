import {
  ChangeDetectionStrategy,
  Component,
  OnInit,
  inject,
  signal,
} from '@angular/core';
import { LogEntry, PetrichorIpc } from '../shared/ipc';

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
  }

  protected async clearLogs(): Promise<void> {
    await this.ipc.clearLogs();
    this.logEntries.set(this.ipc.logEntries());
  }

  protected async emitDemoEvent(): Promise<void> {
    await this.ipc.emitDemoEvent();
    this.logEntries.set(this.ipc.logEntries());
  }
}
