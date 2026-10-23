import { signal } from '@angular/core';
import { TestBed } from '@angular/core/testing';
import { App } from './app';
import {
  ManageSnapshot,
  PackageInfo,
  PetrichorIpc,
  SkinDetails,
} from '../shared/ipc';

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
  capabilities: [
    {
      id: 'mpris',
      label: 'MPRIS',
      available: true,
      fallback: false,
      detail: 'MPRIS actif',
      hint: null,
    },
  ],
};

const packageInfo: PackageInfo = {
  path: '/tmp/package.zip',
  metadata: {
    name: 'Horloge Été',
    author: 'Alice',
    version: '1.0',
    load: '',
    merge_skins: true,
    variable_files: ['Variables.inc'],
    minimum_petrichor: '0.1.0',
    banner: 'Banner.png',
  },
  components: { Skins: 2 },
  files: 2,
  bytes: 512,
  banner_data: 'data:image/png;base64,iVBORw0KGgo=',
};

const details: SkinDetails = {
  path: '/tmp/skins/Clock/Clock.ini',
  name: 'Clock',
  folder: '/tmp/skins/Clock',
  metadata: {
    name: 'Clock',
    author: 'Alice',
    version: '1.0',
    license: null,
    information: null,
  },
  state: {
    window_x: 0,
    window_y: 0,
    anchor_x: 0,
    anchor_y: 0,
    start_hidden: false,
    always_on_top: -1,
    click_through: false,
    draggable: true,
    keep_on_screen: false,
    snap_edges: false,
    save_position: false,
    auto_select_screen: false,
    on_hover: 0,
    alpha_value: 255,
    fade_duration: 0,
    active: 1,
    load_order: 0,
    group: [],
    drag_group: [],
    defaults: {},
  },
  favorites: false,
  open_label: null,
  compat: {
    skin: 'Clock',
    entries: [
      {
        feature: 'font',
        requested: 'Segoe UI',
        resolved: 'Cantarell',
        level: 'automatic',
        plan: 'B',
        message: 'Police remplacée.',
        action: null,
      },
      {
        feature: 'command',
        requested: 'tasklist',
        resolved: 'ps',
        level: 'manual',
        plan: 'A',
        message: null,
        action: 'Vérifiez la commande.',
      },
    ],
  },
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
    inspectPackage: async () => packageInfo,
    skinDetails: async () => details,
    exportCompatibilityReport: async () => '/tmp/report.json',
    previewSkinMigration: async () => [
      {
        line: 2,
        key: 'FontFace',
        before: 'Segoe UI',
        after: 'Cantarell',
      },
    ],
    migrateSkin: async () => ({
      path: details.path,
      backup: `${details.path}.petrichor.bak`,
      changes: [],
    }),
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
    expect(element.textContent).toContain('Ouvrir le créateur');

    const install = Array.from(
      element.querySelectorAll<HTMLButtonElement>('.package-actions button'),
    ).find((button) => button.textContent?.includes('installateur'));
    install?.click();
    fixture.detectChanges();
    expect(element.querySelector('.package-dialog')).toBeTruthy();

    const input = element.querySelector<HTMLInputElement>(
      '.package-dialog input[type="text"]',
    );
    input!.value = '/tmp/package.zip';
    input!.dispatchEvent(new Event('input'));
    const inspect = Array.from(
      element.querySelectorAll<HTMLButtonElement>('.package-dialog button'),
    ).find((button) => button.textContent?.trim() === 'Inspecter');
    inspect?.click();
    await fixture.whenStable();
    fixture.detectChanges();
    expect(element.querySelector('img.package-banner')).toBeTruthy();
    expect(element.textContent).toContain('Variables.inc');
    expect(element.textContent).toContain('Horloge Été');
  });

  it('shows compatibility entries and system capabilities', async () => {
    TestBed.configureTestingModule({
      imports: [App],
      providers: [{ provide: PetrichorIpc, useValue: fakeIpc() }],
    });
    const fixture = TestBed.createComponent(App);
    await fixture.whenStable();
    fixture.detectChanges();
    const element = fixture.nativeElement as HTMLElement;

    element.querySelector<HTMLButtonElement>('.skin-row .link')?.click();
    await fixture.whenStable();
    fixture.detectChanges();
    expect(element.textContent).toContain('2 adaptation(s)');
    expect(element.textContent).toContain('Cantarell');
    expect(element.textContent).toContain('Plan B');
    expect(element.textContent).toContain('Migrer les chemins');

    const tabs = Array.from(
      element.querySelectorAll<HTMLButtonElement>('.tabs button'),
    );
    tabs.find((button) => button.textContent?.trim() === 'Réglages')?.click();
    fixture.detectChanges();
    expect(element.textContent).toContain('MPRIS actif');
  });

  it('opens the package creator with Info, Options and Advanced tabs', async () => {
    TestBed.configureTestingModule({
      imports: [App],
      providers: [{ provide: PetrichorIpc, useValue: fakeIpc() }],
    });
    const fixture = TestBed.createComponent(App);
    await fixture.whenStable();
    fixture.detectChanges();
    const element = fixture.nativeElement as HTMLElement;

    const tabs = Array.from(
      element.querySelectorAll<HTMLButtonElement>('.tabs button'),
    );
    tabs.find((button) => button.textContent?.trim() === 'Paquets')?.click();
    fixture.detectChanges();

    const create = Array.from(
      element.querySelectorAll<HTMLButtonElement>('.package-actions button'),
    ).find((button) => button.textContent?.includes('créateur'));
    create?.click();
    fixture.detectChanges();
    expect(element.textContent).toContain('Créer un paquet');
    expect(element.querySelector('.dialog-tabs')).toBeTruthy();

    const options = Array.from(
      element.querySelectorAll<HTMLButtonElement>('.dialog-tabs button'),
    ).find((button) => button.textContent?.trim() === 'Options');
    options?.click();
    fixture.detectChanges();
    expect(element.textContent).toContain('Fichiers de variables');
    expect(element.textContent).toContain('Fusion par défaut');

    const advanced = Array.from(
      element.querySelectorAll<HTMLButtonElement>('.dialog-tabs button'),
    ).find((button) => button.textContent?.trim() === 'Avancé');
    advanced?.click();
    fixture.detectChanges();
    expect(element.textContent).toContain('Inclure les polices');
    expect(element.textContent).toContain('enveloppe de validation SHA-256');
  });
});
