import { MeterBox, SkinLayout } from './layout';
import {
  Background,
  BarMeter,
  Color,
  ImageMeter,
  RenderModel,
  StringMeter,
  cssColor,
} from './skin-model';

export type ImageSourceFn = (path: string) => CanvasImageSource | null;

export function renderSkin(
  ctx: CanvasRenderingContext2D,
  model: RenderModel,
  layout: SkinLayout,
  getImage: ImageSourceFn = () => null,
): void {
  ctx.clearRect(0, 0, layout.width, layout.height);
  drawBackground(ctx, model.background, layout, getImage);

  for (const box of layout.boxes) {
    drawMeterBackground(ctx, box);
    drawMeter(ctx, box, getImage);
  }
}

function drawBackground(
  ctx: CanvasRenderingContext2D,
  background: Background,
  layout: SkinLayout,
  getImage: ImageSourceFn,
): void {
  paintFill(
    ctx,
    background.color,
    background.color2,
    background.gradient_angle,
    {
      x: 0,
      y: 0,
      width: layout.width,
      height: layout.height,
    },
  );

  if (background.image) {
    const image = getImage(background.image);
    if (image) {
      drawBackgroundImage(ctx, background, image, layout);
    }
  }
}

function drawBackgroundImage(
  ctx: CanvasRenderingContext2D,
  background: Background,
  image: CanvasImageSource,
  layout: SkinLayout,
): void {
  const width = imageWidth(image);
  const height = imageHeight(image);
  if (width <= 0 || height <= 0) return;

  switch (background.mode) {
    case 'SOLID':
      break;
    case 'COPY':
      ctx.drawImage(image, 0, 0);
      break;
    case 'TILED_IMAGE':
      for (let y = 0; y < layout.height; y += height) {
        for (let x = 0; x < layout.width; x += width) {
          ctx.drawImage(image, x, y);
        }
      }
      break;
    case 'SCALED_IMAGE':
      ctx.drawImage(image, 0, 0, layout.width, layout.height);
      break;
    default:
      ctx.drawImage(image, 0, 0);
      break;
  }
}

function drawMeterBackground(
  ctx: CanvasRenderingContext2D,
  box: MeterBox,
): void {
  const { meter } = box;
  if (meter.hidden || box.width <= 0 || box.height <= 0) return;
  if (!meter.solid_color && !meter.solid_color2) return;
  paintFill(
    ctx,
    meter.solid_color ?? { r: 0, g: 0, b: 0, a: 0 },
    meter.solid_color2,
    meter.gradient_angle,
    box,
  );
}

function drawMeter(
  ctx: CanvasRenderingContext2D,
  box: MeterBox,
  getImage: ImageSourceFn,
): void {
  switch (box.meter.type) {
    case 'string':
      drawString(ctx, box);
      break;
    case 'image':
      drawImageMeter(ctx, box, getImage);
      break;
    case 'bar':
      drawBar(ctx, box);
      break;
    default:
      break;
  }
}

function drawString(ctx: CanvasRenderingContext2D, box: MeterBox): void {
  const meter = box.meter as StringMeter;
  if (!box.text) return;

  ctx.save();
  ctx.font = fontString(meter);
  ctx.fillStyle = cssColor(meter.font_color);
  ctx.textAlign = box.text.align;
  ctx.textBaseline = box.text.baseline;

  if (meter.angle) {
    const centerX = box.x + box.width / 2;
    const centerY = box.y + box.height / 2;
    ctx.translate(centerX, centerY);
    ctx.rotate((meter.angle * Math.PI) / 180);
    ctx.translate(-centerX, -centerY);
  }

  ctx.fillText(box.text.value, box.text.x, box.text.y);
  ctx.restore();
}

function drawImageMeter(
  ctx: CanvasRenderingContext2D,
  box: MeterBox,
  getImage: ImageSourceFn,
): void {
  const meter = box.meter as ImageMeter;
  if (!meter.image) return;
  const image = getImage(meter.image);
  if (!image) return;

  const width = imageWidth(image);
  const height = imageHeight(image);
  if (width <= 0 || height <= 0) return;

  const target = box.content;

  if (meter.tile) {
    for (let y = target.y; y < target.y + target.height; y += height) {
      for (let x = target.x; x < target.x + target.width; x += width) {
        ctx.drawImage(image, x, y);
      }
    }
    return;
  }

  if (
    meter.preserve_aspect_ratio > 0 &&
    target.width > 0 &&
    target.height > 0
  ) {
    const scale = Math.min(target.width / width, target.height / height);
    const drawWidth = width * scale;
    const drawHeight = height * scale;
    ctx.drawImage(
      image,
      target.x + (target.width - drawWidth) / 2,
      target.y + (target.height - drawHeight) / 2,
      drawWidth,
      drawHeight,
    );
    return;
  }

  ctx.drawImage(
    image,
    target.x,
    target.y,
    target.width || width,
    target.height || height,
  );
}

function drawBar(ctx: CanvasRenderingContext2D, box: MeterBox): void {
  const meter = box.meter as BarMeter;
  const value = Math.min(Math.max(meter.value, 0), 1);
  const target = box.content;
  const horizontal = meter.orientation !== 'VERTICAL';

  if (horizontal) {
    const filled = target.width * value;
    const startX = meter.flip ? target.x + target.width - filled : target.x;
    ctx.fillStyle = cssColor(meter.color);
    ctx.fillRect(startX, target.y, filled, target.height);
  } else {
    const filled = target.height * value;
    const startY = meter.flip ? target.y : target.y + target.height - filled;
    ctx.fillStyle = cssColor(meter.color);
    ctx.fillRect(target.x, startY, target.width, filled);
  }

  if (meter.border > 0) {
    ctx.lineWidth = meter.border;
    ctx.strokeStyle = cssColor(meter.color);
    ctx.strokeRect(
      target.x + meter.border / 2,
      target.y + meter.border / 2,
      Math.max(0, target.width - meter.border),
      Math.max(0, target.height - meter.border),
    );
  }
}

function paintFill(
  ctx: CanvasRenderingContext2D,
  color: Color,
  color2: Color | null,
  angle: number,
  rect: { x: number; y: number; width: number; height: number },
): void {
  if (!color2) {
    if (color.a <= 0) return;
    ctx.fillStyle = cssColor(color);
    ctx.fillRect(rect.x, rect.y, rect.width, rect.height);
    return;
  }

  const radians = ((angle - 90) * Math.PI) / 180;
  const centerX = rect.x + rect.width / 2;
  const centerY = rect.y + rect.height / 2;
  const length =
    (Math.abs(rect.width * Math.cos(radians)) +
      Math.abs(rect.height * Math.sin(radians))) /
    2;
  const gradient = ctx.createLinearGradient(
    centerX - Math.cos(radians) * length,
    centerY - Math.sin(radians) * length,
    centerX + Math.cos(radians) * length,
    centerY + Math.sin(radians) * length,
  );
  gradient.addColorStop(0, cssColor(color));
  gradient.addColorStop(1, cssColor(color2));
  ctx.fillStyle = gradient;
  ctx.fillRect(rect.x, rect.y, rect.width, rect.height);
}

export function fontString(meter: StringMeter): string {
  const style = meter.string_style.toUpperCase();
  const italic = style.includes('ITALIC') ? 'italic ' : '';
  const weight =
    meter.font_weight > 0 || style.includes('BOLD')
      ? `${meter.font_weight > 0 ? meter.font_weight : 700}`
      : 'normal';
  return `${italic}${weight} ${meter.font_size}px "${meter.font_face}", sans-serif`;
}

function imageWidth(image: CanvasImageSource): number {
  const element = image as HTMLImageElement;
  return element.naturalWidth || element.width || 0;
}

function imageHeight(image: CanvasImageSource): number {
  const element = image as HTMLImageElement;
  return element.naturalHeight || element.height || 0;
}
