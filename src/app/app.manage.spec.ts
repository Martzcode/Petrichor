import { signal } from '@angular/core';
import { TestBed } from '@angular/core/testing';
import { App } from './app';
import { ManageSnapshot, PetrichorIpc } from '../shared/ipc';

const snapshot: ManageSnapshot = {
  version: '0.0.0-test',
  skins: {
    root: '/tmp/skins',
    folders: [
      {
        name: 'Clock',
        path: '/tmp/skins/Clock',
        level: 1,
        files: [
          {
            filename: 'Clock.ini',
            path: '/tmp/skins/Clock/Clock.ini',
            is_favorite: false,
          },
          {
            filename: 'Clock-dark.ini',
            path: '/tmp/skins/Clock/Clock-dark.ini',
            is_favorite: false,
          },
        ],
        base_id: 1,
        active_variant: 0,
        has_favorite: false,
      },
    ],
  },
  windows: [],
  layouts: [],
  settings: {
    debug: false,
    logging: false,
    net_in_speed: 1_000_000,
    net_out_speed: 1_000_000,
    disable_dragging: false,
    selected_color: null,
    skin_editor: null,
    tray_execute_r: null,
    tray_execute_m: null,
    tray_execute_dr: null,
    tray_execute_dm: null,
    disable_version_check: false,
    disable_auto_update: false,
    update_url: null,
    hardware_acceleration: 2,
    skin_path: '/tmp/skins',
    language: null,
    tray_icon: true,
    normal_stay_desktop: false,
  },
  game_mode: {
    active: false,
    fullscreen: false,
    process: false,
    config: {
      start_action: '',
      stop_action: '',
      detect_fullscreen: false,
      detect_process: false,
      process_list: [],
    },
  },
  paths: { config: null, skins: '/tmp/skins', log: null },
  monitors: [],
};

function fakeIpc() {
  const backend = signal(true);
  const logs = signal([]);
  return {
    hasBackend: backend.asReadonly(),
    appVersion: signal('0.0.0-test').asReadonly(),
    logEntries: logs.asReadonly(),
    refreshVersion: async () => undefined,
    refreshSystem: async () => undefined,
    refreshLogs: async () => undefined,
    onLog: async () => undefined,
    onPackageProgress: async () => undefined,
    manageSnapshot: async () => snapshot,
  };
}

describe('Manage UI', () => {
  it('renders skins, variants and tabs from the backend snapshot', async () => {
    TestBed.configureTestingModule({
      imports: [App],
      providers: [{ provide: PetrichorIpc, useValue: fakeIpc() }],
    });
    const fixture = TestBed.createComponent(App);
    await fixture.whenStable();
    fixture.detectChanges();
    const element = fixture.nativeElement as HTMLElement;
    expect(element.textContent).toContain('Clock-dark.ini');
    expect(element.querySelector('.variant select')).toBeTruthy();

    const buttons = Array.from(
      element.querySelectorAll<HTMLButtonElement>('.tabs button'),
    );
    buttons.find((button) => button.textContent?.trim() === 'Layouts')?.click();
    fixture.detectChanges();
    expect(element.textContent).toContain('Aucun layout enregistré.');

    buttons.find((button) => button.textContent?.trim() === 'Paquets')?.click();
    fixture.detectChanges();
    expect(element.textContent).toContain('Installer un paquet');
    expect(element.textContent).toContain('Fichiers de variables');
  });
});
