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
import { layoutSkin, SkinLayout } from '../renderer/layout';
import { fontString, renderSkin } from '../renderer/renderer';
import { ImageMeter, RenderModel, StringMeter } from '../renderer/skin-model';
import { PetrichorIpc } from '../shared/ipc';

@Component({
  selector: 'app-skin-view',
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
  private images = new Map<string, HTMLImageElement>();
  private timer: ReturnType<typeof setInterval> | null = null;
  private updating = false;

  async ngAfterViewInit(): Promise<void> {
    try {
      await this.run();
    } catch (cause) {
      this.error.set(cause instanceof Error ? cause.message : String(cause));
    }
  }

  ngOnDestroy(): void {
    if (this.timer !== null) {
      clearInterval(this.timer);
      this.timer = null;
    }
  }

  private async run(): Promise<void> {
    const label = await this.ipc.currentWindowLabel();
    const info = await this.ipc.skinWindowInfo(label);
    if (!info) {
      this.error.set(`Fenêtre de skin inconnue : ${label}`);
      return;
    }

    const result = await this.ipc.loadRenderModel(info.path);
    if (!result) {
      this.error.set('Backend Petrichor indisponible.');
      return;
    }

    this.label = label;
    this.path = info.path;
    this.model = result.model;

    await this.loadFonts(result.model);
    this.images = await this.loadImages(result.model);

    const layout = this.render();
    await this.ipc.applySkinWindowFlags(label, result.flags);
    if (result.model.dynamic_window_size) {
      await this.ipc.fitSkinWindow(label, layout.width, layout.height);
    }
    await this.ipc.showSkinWindow(label);

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
      if (this.model.dynamic_window_size) {
        await this.ipc.fitSkinWindow(this.label, layout.width, layout.height);
      }
    } finally {
      this.updating = false;
    }
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
    canvas.width = Math.max(1, Math.round(layout.width * ratio));
    canvas.height = Math.max(1, Math.round(layout.height * ratio));
    canvas.style.width = `${layout.width}px`;
    canvas.style.height = `${layout.height}px`;
    ctx.setTransform(ratio, 0, 0, ratio, 0, 0);
  }
}

function fontFamily(path: string): string {
  const file = path.split('/').pop() ?? 'local';
  return file.replace(/\.(ttf|otf|ttc|woff2?)$/i, '');
}
