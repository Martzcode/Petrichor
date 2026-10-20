import {
  AfterViewInit,
  ChangeDetectionStrategy,
  Component,
  ElementRef,
  OnDestroy,
  inject,
  signal,
  viewChild,
} from '@angular/core';
import { getCurrentWindow } from '@tauri-apps/api/window';
import { hitTest, layoutSkin, MeterBox, SkinLayout } from '../renderer/layout';
import { fontString, renderSkin } from '../renderer/renderer';
import { ImageMeter, RenderModel, StringMeter } from '../renderer/skin-model';
import { isTauri, PetrichorIpc, SkinCommand } from '../shared/ipc';

@Component({
  selector: 'app-root',
  templateUrl: './skin-view.html',
  styleUrl: './skin-view.css',
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class SkinView implements AfterViewInit, OnDestroy {
  private readonly ipc = inject(PetrichorIpc);
  private readonly canvasRef =
    viewChild<ElementRef<HTMLCanvasElement>>('canvas');
  protected readonly error = signal<string | null>(null);

  private label = '';
  private path = '';
  private model: RenderModel | null = null;
  private layout: SkinLayout | null = null;
  private images = new Map<string, HTMLImageElement>();
  private timer: ReturnType<typeof setInterval> | null = null;
  private watchdog: ReturnType<typeof setTimeout> | null = null;
  private unlistenCommand: (() => void) | null = null;
  private updating = false;
  private draggable = false;
  private hoveredBox: MeterBox | null = null;
  private pressPosition: { x: number; y: number } | null = null;
  private dragging = false;
  private lastSize: { width: number; height: number } | null = null;

  async ngAfterViewInit(): Promise<void> {
    try {
      this.label = await this.ipc.currentWindowLabel();
      await this.ipc.logBackend('notice', `SkinView démarré (${this.label})`);
      this.unlistenCommand = await this.ipc.onCommand(
        (command) => void this.handleCommand(command),
      );
      this.watchdog = setTimeout(() => void this.showWindow(), 3000);
      await this.run();
    } catch (cause) {
      const message = cause instanceof Error ? cause.message : String(cause);
      this.error.set(message);
      await this.ipc.logBackend(
        'error',
        `SkinView ${this.label || '?'} : ${message}`,
      );
      await this.showWindow();
    } finally {
      this.clearWatchdog();
    }
  }

  ngOnDestroy(): void {
    this.clearWatchdog();
    this.stopUpdates();
    this.unlistenCommand?.();
    this.unlistenCommand = null;
  }

  private clearWatchdog(): void {
    if (this.watchdog !== null) {
      clearTimeout(this.watchdog);
      this.watchdog = null;
    }
  }

  private stopUpdates(): void {
    if (this.timer !== null) {
      clearInterval(this.timer);
      this.timer = null;
    }
  }

  private async showWindow(): Promise<void> {
    if (!this.label) return;
    try {
      await this.ipc.showSkinWindow(this.label);
    } catch {
      return;
    }
  }

  private async run(): Promise<void> {
    this.stopUpdates();
    const label = this.label;
    const info = await this.ipc.skinWindowInfo(label);
    if (!info) {
      throw new Error(`Fenêtre de skin inconnue : ${label}`);
    }

    const result = await this.ipc.loadRenderModel(info.path);
    if (!result) {
      throw new Error('Backend Petrichor indisponible.');
    }

    this.path = info.path;
    this.model = result.model;
    this.draggable = result.flags.draggable;

    await this.loadFonts(result.model);
    this.images = await this.loadImages(result.model);

    const layout = this.render();
    this.lastSize = { width: layout.width, height: layout.height };
    await this.ipc.applySkinWindowFlags(label, result.flags);
    if (result.model.dynamic_window_size) {
      await this.ipc.fitSkinWindow(label, layout.width, layout.height);
    }
    await this.ipc.logBackend(
      'notice',
      `Skin ${result.model.name} rendu (${layout.width}×${layout.height})`,
    );
    await this.showWindow();
    if (result.model.on_refresh_action) {
      await this.execute(result.model.on_refresh_action);
    }

    this.startUpdates(result.model.update_ms);
  }

  private startUpdates(updateMs: number): void {
    if (updateMs <= 0) return;
    this.timer = setInterval(() => void this.tick(), Math.max(50, updateMs));
  }

  private async tick(): Promise<void> {
    if (this.updating || !this.model) return;
    this.updating = true;
    try {
      this.model.measure_values = await this.ipc.readMeasures(this.path);
      const layout = this.render();
      if (
        this.model.dynamic_window_size &&
        (this.lastSize?.width !== layout.width ||
          this.lastSize?.height !== layout.height)
      ) {
        this.lastSize = { width: layout.width, height: layout.height };
        await this.ipc.fitSkinWindow(this.label, layout.width, layout.height);
      }
    } finally {
      this.updating = false;
    }
  }

  private async handleCommand(command: SkinCommand): Promise<void> {
    try {
      switch (command.command) {
        case 'redraw':
          if (this.model) this.render();
          break;
        case 'update':
          await this.tick();
          break;
        case 'refresh':
          await this.run();
          break;
        case 'draggable':
          this.draggable = command.value;
          this.updateCursor(this.hoveredBox);
          break;
      }
    } catch (cause) {
      await this.logError('commande', cause);
    }
  }

  onMouseMove(event: MouseEvent): void {
    const box = this.boxAt(event);
    const name = box?.meter.name ?? null;
    const previous = this.hoveredBox?.meter.name ?? null;

    if (name !== previous) {
      const leaving = this.hoveredBox?.meter.mouse_leave_action;
      if (leaving) void this.execute(leaving);
      this.hoveredBox = box;
      const entering = box?.meter.mouse_over_action;
      if (entering) void this.execute(entering);
    }

    this.updateCursor(box);
  }

  onMouseLeave(): void {
    const leaving = this.hoveredBox?.meter.mouse_leave_action;
    if (leaving) void this.execute(leaving);
    this.hoveredBox = null;
    this.updateCursor(null);
  }

  onMouseDown(event: MouseEvent): void {
    if (event.button !== 0) return;
    this.pressPosition = { x: event.clientX, y: event.clientY };
    if (!this.draggable) return;
    this.dragging = true;
    void this.startDragging();
    void this.ipc.setSkinDragging(this.label, true);
  }

  onMouseUp(event: MouseEvent): void {
    const press = this.pressPosition;
    this.pressPosition = null;
    const wasDragging = this.dragging;
    this.dragging = false;

    if (event.button !== 0) return;
    if (wasDragging) {
      void this.ipc.setSkinDragging(this.label, false);
    }
    const moved =
      press !== null &&
      Math.hypot(event.clientX - press.x, event.clientY - press.y) > 4;
    if (wasDragging && moved) {
      void this.savePosition();
      return;
    }

    const action = this.boxAt(event)?.meter.left_mouse_up_action;
    if (action) void this.execute(action);
  }

  private boxAt(event: MouseEvent): MeterBox | null {
    const layout = this.layout;
    const canvas = this.canvasRef()?.nativeElement;
    if (!layout || !canvas) return null;

    const rect = canvas.getBoundingClientRect();
    return hitTest(
      layout.boxes,
      event.clientX - rect.left,
      event.clientY - rect.top,
    );
  }

  private updateCursor(box: MeterBox | null): void {
    const canvas = this.canvasRef()?.nativeElement;
    if (!canvas) return;

    const meter = box?.meter;
    const hasAction =
      meter?.left_mouse_up_action ||
      meter?.mouse_over_action ||
      meter?.mouse_leave_action;
    canvas.style.cursor = hasAction
      ? 'pointer'
      : this.draggable
        ? 'move'
        : 'default';
  }

  private async savePosition(): Promise<void> {
    try {
      await this.ipc.saveSkinPosition(this.label);
    } catch (cause) {
      await this.logError('position', cause);
    }
  }

  private async startDragging(): Promise<void> {
    if (!isTauri()) return;
    try {
      await getCurrentWindow().startDragging();
    } catch (cause) {
      await this.logError('glisser', cause);
    }
  }

  private async execute(command: string): Promise<void> {
    try {
      await this.ipc.executeBang(this.label, command);
    } catch (cause) {
      await this.logError('action', cause);
    }
  }

  private async logError(context: string, cause: unknown): Promise<void> {
    const message = cause instanceof Error ? cause.message : String(cause);
    await this.ipc.logBackend(
      'error',
      `SkinView ${this.label || '?'} (${context}) : ${message}`,
    );
  }

  private render(): SkinLayout {
    const model = this.model;
    const canvas = this.canvasRef()?.nativeElement;
    const ctx = canvas?.getContext('2d');
    if (!model || !canvas || !ctx) {
      throw new Error('Contexte de rendu 2D indisponible.');
    }

    const layout = layoutSkin(
      model,
      (meter, text) => this.measure(ctx, meter, text),
      (meter) => this.imageSize(this.images, meter),
    );

    this.layout = layout;
    this.prepareCanvas(canvas, ctx, layout);
    renderSkin(ctx, model, layout, (path) => this.images.get(path) ?? null);
    return layout;
  }

  private measure(
    ctx: CanvasRenderingContext2D,
    meter: StringMeter,
    text: string,
  ): { width: number; height: number } {
    ctx.save();
    ctx.font = fontString(meter);
    const metrics = ctx.measureText(text || ' ');
    ctx.restore();
    const ascent = metrics.actualBoundingBoxAscent || 0;
    const descent = metrics.actualBoundingBoxDescent || 0;
    return {
      width: metrics.width,
      height: ascent + descent || meter.font_size * 1.2,
    };
  }

  private imageSize(
    images: Map<string, HTMLImageElement>,
    meter: ImageMeter,
  ): { width: number; height: number } | null {
    if (!meter.image) return null;
    const image = images.get(meter.image);
    if (!image) return null;
    return {
      width: image.naturalWidth || image.width,
      height: image.naturalHeight || image.height,
    };
  }

  private async loadFonts(model: RenderModel): Promise<void> {
    for (const path of model.local_fonts) {
      try {
        const url = await this.ipc.assetUrl(path);
        const family = fontFamily(path);
        const face = new FontFace(family, `url("${url}")`);
        await face.load();
        document.fonts.add(face);
      } catch {
        continue;
      }
    }
  }

  private async loadImages(
    model: RenderModel,
  ): Promise<Map<string, HTMLImageElement>> {
    const paths = new Set<string>();
    if (model.background.image) paths.add(model.background.image);
    for (const meter of model.meters) {
      if (meter.type === 'image' && meter.image) paths.add(meter.image);
    }

    const images = new Map<string, HTMLImageElement>();
    await Promise.all(
      [...paths].map(async (path) => {
        try {
          const url = await this.ipc.assetUrl(path);
          const image = new Image();
          image.src = url;
          await image.decode();
          images.set(path, image);
        } catch {
          return;
        }
      }),
    );
    return images;
  }

  private prepareCanvas(
    canvas: HTMLCanvasElement,
    ctx: CanvasRenderingContext2D,
    layout: SkinLayout,
  ): void {
    const ratio = window.devicePixelRatio || 1;
    const width = Math.max(1, Math.round(layout.width * ratio));
    const height = Math.max(1, Math.round(layout.height * ratio));

    if (canvas.width !== width || canvas.height !== height) {
      canvas.width = width;
      canvas.height = height;
    }
    if (canvas.style.width !== `${layout.width}px`) {
      canvas.style.width = `${layout.width}px`;
    }
    if (canvas.style.height !== `${layout.height}px`) {
      canvas.style.height = `${layout.height}px`;
    }
    ctx.setTransform(ratio, 0, 0, ratio, 0, 0);
  }
}

function fontFamily(path: string): string {
  const file = path.split('/').pop() ?? 'local';
  return file.replace(/\.(ttf|otf|ttc|woff2?)$/i, '');
}
