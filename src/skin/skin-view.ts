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
import {
  hitTest,
  layoutSkin,
  MeterBox,
  SizedMeter,
  SkinLayout,
} from '../renderer/layout';
import { fontString, MeterTransition, renderSkin } from '../renderer/renderer';
import { ButtonMeter, RenderModel, StringMeter } from '../renderer/skin-model';
import {
  appendHistories,
  MeterHistories,
  resetHistories,
  updateMeterValues,
} from '../renderer/values';
import { TextStyle } from '../renderer/inline';
import {
  isTauri,
  MouseActionState,
  PetrichorIpc,
  SkinCommand,
} from '../shared/ipc';
import {
  buttonAction,
  cursorName,
  expandMouseVariables,
  isActionSuppressed,
  MouseStates,
  normalizeAction,
} from './mouse';

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
  protected readonly tooltip = signal<TooltipState | null>(null);
  protected readonly selectionActive = signal(false);
  protected readonly selectionSelected = signal(false);
  protected readonly selectionColor = signal('#38bdf8');
  protected readonly hoverHidden = signal(false);

  private label = '';
  private path = '';
  private model: RenderModel | null = null;
  private layout: SkinLayout | null = null;
  private images = new Map<string, HTMLImageElement>();
  private alphaMaps = new Map<string, ImageData | null>();
  private histories: MeterHistories = new Map();
  private transitions = new Map<string, MeterTransition>();
  private pressedMeter: string | null = null;
  private timer: ReturnType<typeof setInterval> | null = null;
  private watchdog: ReturnType<typeof setTimeout> | null = null;
  private unlistenCommand: (() => void) | null = null;
  private updating = false;
  private draggable = false;
  private hoveredBox: MeterBox | null = null;
  private pressPosition: { x: number; y: number } | null = null;
  private dragging = false;
  private lastSize: { width: number; height: number } | null = null;
  private readonly mouseStates = new Map<
    string,
    Map<string, MouseActionState>
  >();
  private baseOpacity = 1;
  private onHover = 0;
  private mouseX = 0;
  private mouseY = 0;
  private fadeTimer: ReturnType<typeof setTimeout> | null = null;

  async ngAfterViewInit(): Promise<void> {
    try {
      this.label = await this.ipc.currentWindowLabel();
      await this.ipc.logBackend('notice', `SkinView démarré (${this.label})`);
      this.unlistenCommand = await this.ipc.onCommand(
        (command) => void this.handleCommand(command),
      );
      this.watchdog = setTimeout(() => void this.showWindow(), 3000);
      await this.run();
      if (await this.ipc.selectionActive()) this.selectionActive.set(true);
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
    this.clearFade();
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
    this.onHover = Math.min(2, Math.max(0, result.flags.on_hover ?? 0));
    this.hoverHidden.set(false);
    this.clearFade();
    this.baseOpacity =
      Math.min(255, Math.max(0, result.flags.opacity ?? 255)) / 255;
    this.setCanvasOpacity(this.baseOpacity);
    this.applyBlur(result.model.blur === true);

    await this.loadFonts(result.model);
    this.images = await this.loadImages(result.model);
    this.alphaMaps.clear();
    this.transitions.clear();
    resetHistories(this.histories);
    appendHistories(this.histories, result.model);

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
      const snapshot = await this.ipc.readMeasures(this.path);
      this.model.measure_values = snapshot.strings;
      updateMeterValues(this.model, snapshot.values);
      appendHistories(this.histories, this.model);
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
        case 'mouseActions':
          this.updateMouseActions(
            command.target,
            command.actions,
            command.state,
          );
          break;
        case 'opacity':
          this.baseOpacity = Math.min(255, Math.max(0, command.value)) / 255;
          this.clearFade();
          this.setCanvasOpacity(this.baseOpacity);
          break;
        case 'fade':
          this.runFade(command.show, command.duration_ms);
          break;
        case 'selection':
          this.selectionActive.set(command.active);
          this.selectionSelected.set(command.selected);
          this.selectionColor.set(command.color);
          this.updateCursor(this.hoveredBox);
          break;
        case 'onHover':
          this.onHover = Math.min(2, Math.max(0, command.value));
          this.hoverHidden.set(false);
          this.setCanvasOpacity(this.baseOpacity);
          break;
      }
    } catch (cause) {
      await this.logError('commande', cause);
    }
  }

  onMouseMove(event: MouseEvent): void {
    this.setMousePosition(event);
    const box = this.boxAt(event);
    this.updateTooltip(box, event);
    this.applyHover(event);
    if (this.selectionActive()) {
      this.hoveredBox = box;
      this.updateCursor(box);
      return;
    }
    const name = box?.meter.name ?? null;
    const previous = this.hoveredBox?.meter.name ?? null;

    if (name !== previous) {
      const leaving = this.actionFor(
        this.hoveredBox?.meter,
        'mouseleaveaction',
      );
      if (leaving) void this.execute(leaving);
      this.hoveredBox = box;
      const entering = this.actionFor(box?.meter, 'mouseoveraction');
      if (entering) void this.execute(entering);
    }

    this.updateCursor(box);
  }

  onMouseLeave(): void {
    this.restoreHover();
    if (this.selectionActive()) {
      this.hoveredBox = null;
      this.tooltip.set(null);
      this.updateCursor(null);
      return;
    }
    const leaving = this.actionFor(this.hoveredBox?.meter, 'mouseleaveaction');
    if (leaving) void this.execute(leaving);
    this.hoveredBox = null;
    this.tooltip.set(null);
    if (this.model) this.render();
    this.updateCursor(null);
  }

  onMouseDown(event: MouseEvent): void {
    this.setMousePosition(event);
    if (this.selectionActive()) {
      event.preventDefault();
      if (event.button === 0) {
        void this.startSelectionDrag(event.ctrlKey || event.shiftKey);
      }
      return;
    }
    const box = this.boxAt(event);
    const down = this.actionFor(box?.meter, buttonAction(event.button, 'down'));
    if (down) {
      event.preventDefault();
      void this.execute(down);
    }

    if (event.button !== 0) return;
    this.pressPosition = { x: event.clientX, y: event.clientY };
    this.pressedMeter = box?.meter.name ?? null;
    if (this.pressedMeter && this.model) this.render();
    if (!this.draggable) return;
    this.dragging = true;
    void this.startDragging();
    void this.ipc.setSkinDragging(this.label, true);
  }

  onMouseUp(event: MouseEvent): void {
    this.setMousePosition(event);
    if (this.selectionActive()) {
      if (event.button === 0 && this.dragging) {
        this.dragging = false;
        void this.ipc.setSkinDragging(this.label, false);
      }
      return;
    }
    const press = this.pressPosition;
    const pressedMeter = this.pressedMeter;
    const wasDragging = this.dragging;

    if (event.button === 0) {
      this.pressPosition = null;
      this.pressedMeter = null;
      this.dragging = false;
    }

    const box = this.boxAt(event);
    const moved =
      press !== null &&
      Math.hypot(event.clientX - press.x, event.clientY - press.y) > 4;

    if (event.button === 0) {
      if (wasDragging) {
        void this.ipc.setSkinDragging(this.label, false);
      }
      if (wasDragging && moved) {
        void this.savePosition();
        return;
      }

      const meter = box?.meter;
      if (meter?.type === 'button' && meter.name === pressedMeter) {
        const command = (meter as ButtonMeter).command;
        if (command) void this.execute(command);
      }
      if (this.model) this.render();
    }

    const up = this.actionFor(box?.meter, buttonAction(event.button, 'up'));
    if (up) void this.execute(up);
  }

  onMouseDoubleClick(event: MouseEvent): void {
    if (this.selectionActive()) return;
    this.setMousePosition(event);
    const action = this.actionFor(
      this.boxAt(event)?.meter,
      buttonAction(event.button, 'doubleclick'),
    );
    if (action) {
      event.preventDefault();
      void this.execute(action);
    }
  }

  onMouseScroll(event: WheelEvent): void {
    if (this.selectionActive()) return;
    this.mouseX = event.clientX;
    this.mouseY = event.clientY;
    const key =
      event.deltaY < 0
        ? 'mousescrollupaction'
        : event.deltaY > 0
          ? 'mousescrolldownaction'
          : event.deltaX < 0
            ? 'mousescrollleftaction'
            : event.deltaX > 0
              ? 'mousescrollrightaction'
              : null;
    if (!key) return;
    const action = this.actionFor(this.boxAt(event)?.meter, key);
    if (!action) return;
    event.preventDefault();
    void this.execute(action);
  }

  onContextMenu(event: MouseEvent): void {
    event.preventDefault();
    if (this.selectionActive()) return;
    const meter = this.boxAt(event)?.meter;
    if (
      this.actionFor(meter, 'rightmouseupaction') ||
      this.actionFor(meter, 'rightmousedownaction') ||
      this.actionFor(meter, 'rightmousedoubleclickaction')
    ) {
      return;
    }
    void this.openSkinMenu(event.clientX, event.clientY);
  }

  private async openSkinMenu(x?: number, y?: number): Promise<void> {
    try {
      await this.ipc.showSkinMenu(this.label, x, y);
    } catch (cause) {
      await this.logError('menu', cause);
    }
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
      (box, x, y) => this.buttonOpaque(box, x, y),
    );
  }

  private buttonOpaque(box: MeterBox, x: number, y: number): boolean {
    const meter = box.meter;
    if (meter.type !== 'button' || !meter.image) return true;
    if (meter.solid_color && meter.solid_color.a > 0) return true;
    if (box.content.width <= 0 || box.content.height <= 0) return false;

    const image = this.images.get(meter.image);
    if (!image) return true;
    const width = image.naturalWidth || image.width;
    const height = image.naturalHeight || image.height;
    if (width <= 0 || height <= 0) return true;

    const data = this.alphaData(meter.image, image, width, height);
    if (!data) return true;

    const frameWidth = width / 3;
    const sourceX = Math.min(
      frameWidth - 1,
      Math.max(0, ((x - box.content.x) / box.content.width) * frameWidth),
    );
    const sourceY = Math.min(
      height - 1,
      Math.max(0, ((y - box.content.y) / box.content.height) * height),
    );
    const index = (Math.floor(sourceY) * width + Math.floor(sourceX)) * 4 + 3;
    return (data.data[index] ?? 0) > 0;
  }

  private alphaData(
    path: string,
    image: HTMLImageElement,
    width: number,
    height: number,
  ): ImageData | null {
    if (this.alphaMaps.has(path)) return this.alphaMaps.get(path) ?? null;
    let data: ImageData | null = null;
    try {
      const canvas = document.createElement('canvas');
      canvas.width = width;
      canvas.height = height;
      const context = canvas.getContext('2d');
      if (context) {
        context.drawImage(image, 0, 0, width, height);
        data = context.getImageData(0, 0, width, height);
      }
    } catch {
      data = null;
    }
    this.alphaMaps.set(path, data);
    return data;
  }

  private updateTooltip(box: MeterBox | null, event: MouseEvent): void {
    const meter = box?.meter;
    const text = meter?.tooltip_text;
    if (!meter || !text || meter.tooltip_hidden) {
      this.tooltip.set(null);
      return;
    }

    this.tooltip.set({
      text,
      title: meter.tooltip_title ?? null,
      icon: tooltipIcon(meter.tooltip_icon ?? null),
      width: meter.tooltip_width ?? 1000,
      type: meter.tooltip_type ?? false,
      x: event.clientX + 12,
      y: event.clientY + 16,
    });
  }

  private actionFor(
    meter: RenderModel['meters'][number] | undefined,
    action: string | null,
  ): string | null {
    if (!meter || !action) return null;
    const key = normalizeAction(action);
    const actions = meter.mouse_actions ?? {};
    const value =
      actions[key] ??
      (key === 'leftmouseupaction' ? meter.left_mouse_up_action : null) ??
      (key === 'mouseoveraction' ? meter.mouse_over_action : null) ??
      (key === 'mouseleaveaction' ? meter.mouse_leave_action : null);
    if (!value || this.isActionSuppressed(meter, key)) return null;
    return value;
  }

  private isActionSuppressed(
    meter: RenderModel['meters'][number],
    action: string,
  ): boolean {
    return isActionSuppressed(meter, action, this.mouseStates);
  }

  private updateMouseActions(
    target: string,
    actions: string[],
    state: MouseActionState,
  ): void {
    const key = target.trim().toLocaleLowerCase();
    if (!key) return;
    let states = this.mouseStates.get(key);
    if (!states) {
      states = new Map();
      this.mouseStates.set(key, states);
    }
    for (const action of actions) {
      const name = normalizeAction(action);
      if (!name) continue;
      const next =
        state === 'toggle'
          ? states.get(name) === 'disabled'
            ? 'enabled'
            : 'disabled'
          : state;
      if (next === 'enabled') {
        states.delete(name);
      } else {
        states.set(name, next);
      }
    }
    if (states.size === 0) this.mouseStates.delete(key);
    this.updateCursor(this.hoveredBox);
  }

  private updateCursor(box: MeterBox | null): void {
    const canvas = this.canvasRef()?.nativeElement;
    if (!canvas) return;
    if (this.selectionActive()) {
      canvas.style.cursor = 'pointer';
      return;
    }

    const meter = box?.meter;
    const hasAction =
      (meter &&
        Object.keys(meter.mouse_actions ?? {}).some(
          (name) => !this.isActionSuppressed(meter, name),
        )) ||
      meter?.left_mouse_up_action ||
      meter?.mouse_over_action ||
      meter?.mouse_leave_action ||
      (meter?.type === 'button' && meter.command);
    const cursorEnabled = meter?.mouse_action_cursor !== false;
    canvas.style.cursor =
      hasAction && cursorEnabled
        ? cursorName(meter?.mouse_action_cursor_name ?? null)
        : this.draggable
          ? 'move'
          : 'default';
  }

  private setMousePosition(event: MouseEvent): void {
    this.mouseX = event.clientX;
    this.mouseY = event.clientY;
  }

  private applyHover(event: MouseEvent): void {
    if (this.onHover === 0 || this.hoverHidden()) return;
    if (event.ctrlKey || event.shiftKey || event.altKey) return;
    this.hoverHidden.set(true);
    const canvas = this.canvasRef()?.nativeElement;
    if (!canvas) return;
    if (this.onHover === 1) {
      this.setCanvasOpacity(0);
    } else {
      canvas.style.transition = 'opacity 150ms linear';
      canvas.style.opacity = '0';
    }
  }

  private restoreHover(): void {
    if (!this.hoverHidden()) return;
    this.hoverHidden.set(false);
    this.setCanvasOpacity(this.baseOpacity);
  }

  private setCanvasOpacity(value: number): void {
    const canvas = this.canvasRef()?.nativeElement;
    if (canvas) canvas.style.opacity = String(value);
  }

  private clearFade(): void {
    if (this.fadeTimer !== null) {
      clearTimeout(this.fadeTimer);
      this.fadeTimer = null;
    }
    const canvas = this.canvasRef()?.nativeElement;
    if (canvas) canvas.style.transition = 'none';
  }

  private runFade(show: boolean, durationMs: number): void {
    const canvas = this.canvasRef()?.nativeElement;
    if (!canvas) return;
    this.clearFade();
    const duration = Math.max(0, durationMs);
    canvas.style.transition = 'none';
    canvas.style.opacity = String(show ? 0 : this.baseOpacity);
    void canvas.offsetWidth;
    canvas.style.transition = duration
      ? `opacity ${duration}ms linear`
      : 'none';
    canvas.style.opacity = String(show ? this.baseOpacity : 0);
    if (!show) {
      this.fadeTimer = setTimeout(() => {
        this.fadeTimer = null;
        void this.ipc.hideSkinWindow(this.label);
        canvas.style.transition = 'none';
        canvas.style.opacity = String(this.baseOpacity);
      }, duration + 20);
    }
  }

  private async savePosition(): Promise<void> {
    try {
      await this.ipc.saveSkinPosition(this.label);
    } catch (cause) {
      await this.logError('position', cause);
    }
  }

  private async startSelectionDrag(additive: boolean): Promise<void> {
    try {
      const selected = await this.ipc.selectSkin(this.label, additive);
      if (!selected) return;
      this.dragging = true;
      await this.ipc.setSkinDragging(this.label, true);
      await this.startDragging();
    } catch (cause) {
      this.dragging = false;
      await this.logError('sélection', cause);
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
      await this.ipc.executeBang(
        this.label,
        expandMouseVariables(command, this.mouseX, this.mouseY),
      );
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
    renderSkin(
      ctx,
      model,
      layout,
      (path) => this.images.get(path) ?? null,
      this.histories,
      {
        hovered: this.hoveredBox?.meter.name ?? null,
        pressed: this.pressedMeter,
        transitions: this.transitions,
        now:
          typeof performance === 'undefined' ? Date.now() : performance.now(),
      },
    );
    return layout;
  }

  private measure(
    ctx: CanvasRenderingContext2D,
    meter: StringMeter,
    text: string,
    style?: TextStyle,
  ): { width: number; height: number } {
    ctx.save();
    ctx.font = fontString(meter, style);
    const metrics = ctx.measureText(text || ' ');
    ctx.restore();
    const ascent = metrics.actualBoundingBoxAscent || 0;
    const descent = metrics.actualBoundingBoxDescent || 0;
    return {
      width: metrics.width,
      height: ascent + descent || (style?.size ?? meter.font_size) * 1.2,
    };
  }

  private imageSize(
    images: Map<string, HTMLImageElement>,
    meter: SizedMeter,
  ): { width: number; height: number } | null {
    if (!meter.image) return null;
    const image = images.get(meter.image);
    if (!image) return null;
    return {
      width: image.naturalWidth || image.width,
      height: image.naturalHeight || image.height,
    };
  }

  private applyBlur(enabled: boolean): void {
    const canvas = this.canvasRef()?.nativeElement;
    if (!canvas) return;
    canvas.style.backdropFilter = enabled ? 'blur(8px)' : '';
    canvas.style.setProperty(
      '-webkit-backdrop-filter',
      enabled ? 'blur(8px)' : '',
    );
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
      if (
        (meter.type === 'image' ||
          meter.type === 'rotator' ||
          meter.type === 'bitmap' ||
          meter.type === 'button') &&
        meter.image
      ) {
        paths.add(meter.image);
      }
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

interface TooltipState {
  text: string;
  title: string | null;
  icon: string | null;
  width: number;
  type: boolean;
  x: number;
  y: number;
}

function tooltipIcon(value: string | null): string | null {
  switch ((value ?? '').toUpperCase()) {
    case 'INFO':
      return 'i';
    case 'WARNING':
      return '!';
    case 'ERROR':
      return 'x';
    case 'QUESTION':
      return '?';
    case 'SHIELD':
      return 'S';
    default:
      return null;
  }
}
