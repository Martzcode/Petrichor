import { lookup, MeterBox, PlacedRun, SkinLayout } from './layout';
import { baseStyle, TextStyle } from './inline';
import {
  Background,
  BarMeter,
  BitmapMeter,
  ButtonMeter,
  Color,
  HistogramMeter,
  ImageMeter,
  LineMeter,
  RenderModel,
  RotatorMeter,
  RoundLineMeter,
  ShapeCombine,
  ShapeMeter,
  ShapePart,
  StringMeter,
  cssColor,
} from './skin-model';
import type { MeterHistories } from './values';

export type ImageSourceFn = (path: string) => CanvasImageSource | null;

export interface MeterTransition {
  value: number;
  startValue: number;
  startTime: number;
}

export interface RenderState {
  hovered: string | null;
  pressed: string | null;
  transitions?: Map<string, MeterTransition>;
  now?: number;
  values?: Record<string, string>;
}

const NO_STATE: RenderState = { hovered: null, pressed: null };

export function renderSkin(
  ctx: CanvasRenderingContext2D,
  model: RenderModel,
  layout: SkinLayout,
  getImage: ImageSourceFn = () => null,
  histories: MeterHistories = new Map(),
  state: RenderState = NO_STATE,
): void {
  ctx.clearRect(0, 0, layout.width, layout.height);
  drawBackground(ctx, model.background, layout, getImage);

  for (const box of layout.boxes) {
    if (box.clip && (box.clip.width <= 0 || box.clip.height <= 0)) continue;
    ctx.save();
    if (box.clip) {
      ctx.beginPath();
      ctx.rect(box.clip.x, box.clip.y, box.clip.width, box.clip.height);
      ctx.clip();
    }
    const matrix = box.meter.transformation_matrix;
    if (matrix && matrix.length === 6) {
      ctx.transform(
        matrix[0],
        matrix[1],
        matrix[2],
        matrix[3],
        matrix[4],
        matrix[5],
      );
    }
    if (box.meter.anti_alias !== undefined) {
      ctx.imageSmoothingEnabled = box.meter.anti_alias;
    }
    drawMeterBackground(ctx, box);
    drawMeter(ctx, box, getImage, histories, state);
    ctx.restore();
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
  drawBevel(ctx, box);
}

function drawBevel(ctx: CanvasRenderingContext2D, box: MeterBox): void {
  const kind = (box.meter.bevel_type ?? 'NONE').toUpperCase();
  if (kind !== 'UP' && kind !== 'DOWN') return;
  const light = box.meter.bevel_color ?? { r: 255, g: 255, b: 255, a: 255 };
  const dark = box.meter.bevel_color2 ?? { r: 0, g: 0, b: 0, a: 255 };
  const first = kind === 'UP' ? light : dark;
  const second = kind === 'UP' ? dark : light;
  const x = box.x + 0.5;
  const y = box.y + 0.5;
  const right = box.x + box.width - 0.5;
  const bottom = box.y + box.height - 0.5;

  ctx.lineWidth = 1;
  ctx.strokeStyle = cssColor(first);
  ctx.beginPath();
  ctx.moveTo(x, bottom);
  ctx.lineTo(x, y);
  ctx.lineTo(right, y);
  ctx.stroke();

  ctx.strokeStyle = cssColor(second);
  ctx.beginPath();
  ctx.moveTo(x, bottom);
  ctx.lineTo(right, bottom);
  ctx.lineTo(right, y);
  ctx.stroke();
}

function drawMeter(
  ctx: CanvasRenderingContext2D,
  box: MeterBox,
  getImage: ImageSourceFn,
  histories: MeterHistories,
  state: RenderState,
): void {
  switch (box.meter.type) {
    case 'string':
      drawString(ctx, box);
      break;
    case 'image':
      drawImageMeter(ctx, box, getImage, state);
      break;
    case 'bar':
      drawBar(ctx, box);
      break;
    case 'histogram':
      drawHistogram(ctx, box, getImage, histories);
      break;
    case 'line':
      drawLine(ctx, box, histories);
      break;
    case 'rotator':
      drawRotator(ctx, box, getImage);
      break;
    case 'roundline':
      drawRoundLine(ctx, box);
      break;
    case 'bitmap':
      drawBitmap(ctx, box, getImage, state);
      break;
    case 'button':
      drawButton(ctx, box, getImage, state);
      break;
    case 'shape':
      drawShape(ctx, box);
      break;
    default:
      break;
  }
}

function drawString(ctx: CanvasRenderingContext2D, box: MeterBox): void {
  const meter = box.meter as StringMeter;
  if (!box.text) return;

  ctx.save();
  ctx.textAlign = box.text.runs.length ? 'left' : box.text.align;
  ctx.textBaseline = box.text.baseline;

  if (meter.angle) {
    const centerX = box.x + box.width / 2;
    const centerY = box.y + box.height / 2;
    ctx.translate(centerX, centerY);
    ctx.rotate((meter.angle * Math.PI) / 180);
    ctx.translate(-centerX, -centerY);
  }

  const runs = box.text.runs.length
    ? box.text.runs
    : [
        {
          text: box.text.value,
          style: baseStyle(meter),
          x: box.text.x,
          width: 0,
        },
      ];

  for (const run of runs) {
    drawTextRun(ctx, meter, run, box.text.y, box.text.baseline);
  }
  ctx.restore();
}

function drawTextRun(
  ctx: CanvasRenderingContext2D,
  meter: StringMeter,
  run: PlacedRun,
  baselineY: number,
  baseline: 'top' | 'middle' | 'bottom',
): void {
  ctx.font = fontString(meter, run.style);
  const fill = runFill(ctx, run);
  const text = run.text;
  const spacing = run.style.spacing;
  let textY = baselineY;
  if (baseline === 'top') {
    const metrics = ctx.measureText(text);
    const glyphAscent = metrics.actualBoundingBoxAscent;
    const emAscent = metrics.fontBoundingBoxAscent;
    if (
      glyphAscent !== undefined &&
      emAscent !== undefined &&
      Number.isFinite(glyphAscent) &&
      Number.isFinite(emAscent)
    ) {
      textY -= Math.max(0, emAscent - glyphAscent);
    }
  }

  if (run.style.shadow) {
    const shadow = run.style.shadow;
    ctx.save();
    ctx.fillStyle = cssColor(shadow.color);
    ctx.shadowColor = cssColor(shadow.color);
    ctx.shadowOffsetX = shadow.offsetX;
    ctx.shadowOffsetY = shadow.offsetY;
    ctx.shadowBlur = shadow.blur;
    drawRunText(ctx, text, run, textY, spacing);
    ctx.restore();
  }

  ctx.fillStyle = fill;
  drawRunText(ctx, text, run, textY, spacing);

  if (run.style.underline || run.style.strikethrough) {
    ctx.fillStyle = cssColor(run.style.color);
    const size = run.style.size;
    const top =
      baseline === 'top'
        ? textY
        : baseline === 'middle'
          ? textY - size / 2
          : textY - size;
    const thickness = Math.max(1, size / 14);
    if (run.style.underline) {
      ctx.fillRect(run.x, top + size * 0.94, run.width, thickness);
    }
    if (run.style.strikethrough) {
      ctx.fillRect(run.x, top + size * 0.55, run.width, thickness);
    }
  }
}

function drawRunText(
  ctx: CanvasRenderingContext2D,
  text: string,
  run: PlacedRun,
  baselineY: number,
  spacing: TextStyle['spacing'],
): void {
  if (!spacing) {
    ctx.fillText(text, run.x, baselineY);
    return;
  }

  let x = run.x;
  for (const character of Array.from(text)) {
    const advance = spacing.advance ?? ctx.measureText(character).width;
    ctx.fillText(character, x + spacing.leading, baselineY);
    x += advance + spacing.leading + spacing.trailing;
  }
}

function runFill(
  ctx: CanvasRenderingContext2D,
  run: PlacedRun,
): string | CanvasGradient {
  const gradient = run.style.gradient;
  if (!gradient || !gradient.stops.length) {
    return cssColor(run.style.color);
  }

  const radians = (gradient.angle * Math.PI) / 180;
  const centerX = run.x + run.width / 2;
  const centerY = run.style.size / 2;
  const length = Math.max(1, run.width, run.style.size) / 2;
  const fill = ctx.createLinearGradient(
    centerX - Math.cos(radians) * length,
    centerY - Math.sin(radians) * length,
    centerX + Math.cos(radians) * length,
    centerY + Math.sin(radians) * length,
  );
  for (const stop of gradient.stops) {
    fill.addColorStop(
      Math.min(Math.max(stop.position, 0), 1),
      cssColor(stop.color),
    );
  }
  return fill;
}

function dynamicImagePath(
  meter: ImageMeter,
  values: Record<string, string>,
): string | null {
  const measure = meter.image_measure;
  if (!measure) return null;
  const raw = lookup(values, measure).trim().toLowerCase();
  if (!raw) return null;
  for (const variant of meter.image_variants ?? []) {
    const file = variant.split(/[\\/]/).pop()?.toLowerCase() ?? '';
    const stem = file.replace(/\.[^.]+$/, '');
    if (file === raw || stem === raw) return variant;
  }
  return null;
}

function drawImageMeter(
  ctx: CanvasRenderingContext2D,
  box: MeterBox,
  getImage: ImageSourceFn,
  state: RenderState,
): void {
  const meter = box.meter as ImageMeter;
  const path = meter.image ?? dynamicImagePath(meter, state.values ?? {});
  if (!path) return;
  const loaded = getImage(path);
  if (!loaded) return;

  const width = imageWidth(loaded);
  const height = imageHeight(loaded);
  if (width <= 0 || height <= 0) return;

  const processed = processImage(loaded, width, height, meter);
  const image = processed ?? loaded;
  const target = box.content;
  const flip = (meter.flip ?? 'NONE').toUpperCase();
  const rotate = meter.rotate ?? 0;
  const transformed = flip !== 'NONE' || rotate !== 0;

  ctx.save();
  if (transformed) {
    const centerX = target.x + target.width / 2;
    const centerY = target.y + target.height / 2;
    ctx.translate(centerX, centerY);
    if (flip === 'HORIZONTAL' || flip === 'BOTH') ctx.scale(-1, 1);
    if (flip === 'VERTICAL' || flip === 'BOTH') ctx.scale(1, -1);
    if (rotate !== 0) ctx.rotate((rotate * Math.PI) / 180);
    ctx.translate(-centerX, -centerY);
  }
  if (!processed && meter.greyscale) ctx.filter = 'grayscale(1)';

  paintImage(ctx, image, meter, width, height, target);
  ctx.restore();
}

function processImage(
  image: CanvasImageSource,
  width: number,
  height: number,
  meter: ImageMeter,
): CanvasImageSource | null {
  const matrix = meter.color_matrix;
  if (!meter.tint && !meter.greyscale && !matrix) return null;
  const context = createOffscreenCanvas(width, height);
  if (!context) return null;

  if (meter.greyscale) context.filter = 'grayscale(1)';
  context.drawImage(image, 0, 0, width, height);
  context.filter = 'none';

  if (meter.tint) {
    context.globalCompositeOperation = 'multiply';
    context.fillStyle = cssColor(meter.tint);
    context.fillRect(0, 0, width, height);
    context.globalCompositeOperation = 'destination-in';
    context.drawImage(image, 0, 0, width, height);
    context.globalCompositeOperation = 'source-over';
  }

  if (matrix && matrix.length === 20)
    applyColorMatrix(context, width, height, matrix);

  return context.canvas;
}

function applyColorMatrix(
  ctx: CanvasRenderingContext2D,
  width: number,
  height: number,
  matrix: number[],
): void {
  try {
    const image = ctx.getImageData(0, 0, width, height);
    const data = image.data;
    for (let offset = 0; offset < data.length; offset += 4) {
      const r = data[offset] / 255;
      const g = data[offset + 1] / 255;
      const b = data[offset + 2] / 255;
      const a = data[offset + 3] / 255;
      data[offset] = clampByte(
        (r * matrix[0] +
          g * matrix[4] +
          b * matrix[8] +
          a * matrix[12] +
          matrix[16]) *
          255,
      );
      data[offset + 1] = clampByte(
        (r * matrix[1] +
          g * matrix[5] +
          b * matrix[9] +
          a * matrix[13] +
          matrix[17]) *
          255,
      );
      data[offset + 2] = clampByte(
        (r * matrix[2] +
          g * matrix[6] +
          b * matrix[10] +
          a * matrix[14] +
          matrix[18]) *
          255,
      );
      data[offset + 3] = clampByte(
        (r * matrix[3] +
          g * matrix[7] +
          b * matrix[11] +
          a * matrix[15] +
          matrix[19]) *
          255,
      );
    }
    ctx.putImageData(image, 0, 0);
  } catch {
    return;
  }
}

function clampByte(value: number): number {
  return Math.min(255, Math.max(0, Math.round(value)));
}

function paintImage(
  ctx: CanvasRenderingContext2D,
  image: CanvasImageSource,
  meter: ImageMeter,
  width: number,
  height: number,
  target: { x: number; y: number; width: number; height: number },
): void {
  const crop = meter.crop;
  const source = {
    x: crop?.[0] ?? 0,
    y: crop?.[1] ?? 0,
    width: crop?.[2] ?? width,
    height: crop?.[3] ?? height,
  };

  const [left, top, right, bottom] = meter.scale_margins;
  if (
    left + top + right + bottom > 0 &&
    target.width > 0 &&
    target.height > 0
  ) {
    drawNineSlice(ctx, image, source, target, left, top, right, bottom);
    return;
  }

  if (meter.tile) {
    for (let y = target.y; y < target.y + target.height; y += source.height) {
      for (let x = target.x; x < target.x + target.width; x += source.width) {
        drawImagePart(ctx, image, source, x, y, source.width, source.height);
      }
    }
    return;
  }

  if (
    meter.preserve_aspect_ratio > 0 &&
    target.width > 0 &&
    target.height > 0
  ) {
    const mode = meter.preserve_aspect_ratio;
    const scale =
      mode === 2
        ? Math.max(target.width / source.width, target.height / source.height)
        : Math.min(target.width / source.width, target.height / source.height);
    const drawWidth = source.width * scale;
    const drawHeight = source.height * scale;
    if (mode === 2) {
      ctx.save();
      ctx.beginPath();
      ctx.rect(target.x, target.y, target.width, target.height);
      ctx.clip();
    }
    drawImagePart(
      ctx,
      image,
      source,
      target.x + (target.width - drawWidth) / 2,
      target.y + (target.height - drawHeight) / 2,
      drawWidth,
      drawHeight,
    );
    if (mode === 2) ctx.restore();
    return;
  }

  drawImagePart(
    ctx,
    image,
    source,
    target.x,
    target.y,
    target.width || source.width,
    target.height || source.height,
  );
}

function drawImagePart(
  ctx: CanvasRenderingContext2D,
  image: CanvasImageSource,
  source: { x: number; y: number; width: number; height: number },
  dx: number,
  dy: number,
  dw: number,
  dh: number,
): void {
  if (source.x === 0 && source.y === 0) {
    ctx.drawImage(image, 0, 0, source.width, source.height, dx, dy, dw, dh);
  } else {
    ctx.drawImage(
      image,
      source.x,
      source.y,
      source.width,
      source.height,
      dx,
      dy,
      dw,
      dh,
    );
  }
}

function drawNineSlice(
  ctx: CanvasRenderingContext2D,
  image: CanvasImageSource,
  source: { x: number; y: number; width: number; height: number },
  target: { x: number; y: number; width: number; height: number },
  left: number,
  top: number,
  right: number,
  bottom: number,
): void {
  const width = source.width;
  const height = source.height;
  const l = Math.min(Math.max(0, left), width);
  const t = Math.min(Math.max(0, top), height);
  const r = Math.min(Math.max(0, right), width - l);
  const b = Math.min(Math.max(0, bottom), height - t);

  const dl = Math.min(l, target.width);
  const dt = Math.min(t, target.height);
  const dr = Math.min(r, Math.max(0, target.width - dl));
  const db = Math.min(b, Math.max(0, target.height - dt));

  const sx = [0, l, width - r];
  const sw = [l, width - l - r, r];
  const sy = [0, t, height - b];
  const sh = [t, height - t - b, b];
  const dx = [target.x, target.x + dl, target.x + target.width - dr];
  const dw = [dl, target.width - dl - dr, dr];
  const dy = [target.y, target.y + dt, target.y + target.height - db];
  const dh = [dt, target.height - dt - db, db];

  for (let row = 0; row < 3; row += 1) {
    for (let column = 0; column < 3; column += 1) {
      if (sw[column] <= 0 || sh[row] <= 0 || dw[column] <= 0 || dh[row] <= 0) {
        continue;
      }
      ctx.drawImage(
        image,
        source.x + sx[column],
        source.y + sy[row],
        sw[column],
        sh[row],
        dx[column],
        dy[row],
        dw[column],
        dh[row],
      );
    }
  }
}

function drawShape(ctx: CanvasRenderingContext2D, box: MeterBox): void {
  const meter = box.meter as ShapeMeter;
  const target = box.content;
  if (target.width <= 0 || target.height <= 0) return;

  const hidden = combinedShapes(meter.shapes);
  ctx.save();
  ctx.translate(target.x, target.y);
  for (let index = 0; index < meter.shapes.length; index += 1) {
    const part = meter.shapes[index];
    if (hidden.has(index)) continue;
    if (part.combine) {
      drawCombinedShape(ctx, meter, part, target);
      continue;
    }
    drawShapePart(ctx, part, meter, target);
  }
  ctx.restore();
}

function drawShapePart(
  ctx: CanvasRenderingContext2D,
  part: ShapePart,
  meter: ShapeMeter,
  rect: { x: number; y: number; width: number; height: number },
  mask = false,
): void {
  const fill = part.fill_color ?? meter.fill_color;
  const fill2 = part.fill_color2 ?? meter.fill_color2;
  const stroke = part.stroke_color ?? meter.stroke_color;
  const stroke2 = part.stroke_color2 ?? meter.stroke_color2;
  const width = part.stroke_width ?? meter.stroke_width;
  const dash = part.stroke_dash ?? meter.stroke_dash;

  ctx.save();
  applyShapeTransform(ctx, part);
  ctx.beginPath();
  traceShape(ctx, part);
  const open = isOpenShape(part);
  if (!open && (part.kind === 'arc' || part.kind === 'curve')) {
    ctx.closePath();
  }
  if (mask) {
    if (open) {
      ctx.strokeStyle = '#ffffff';
      ctx.lineWidth = Math.max(1, width);
      ctx.stroke();
    } else {
      ctx.fillStyle = '#ffffff';
      ctx.fill();
    }
  } else if (!open && fill.a > 0) {
    ctx.fillStyle = shapeFill(ctx, fill, fill2, meter.gradient_angle, rect);
    ctx.fill();
  }
  if (!mask && width > 0 && stroke.a > 0) {
    ctx.lineWidth = width;
    ctx.strokeStyle = shapeStroke(
      ctx,
      stroke,
      stroke2,
      meter.gradient_angle,
      rect,
    );
    if (dash.length) ctx.setLineDash(dash);
    ctx.stroke();
    if (dash.length) ctx.setLineDash([]);
  }
  ctx.restore();
}

function combinedShapes(shapes: ShapePart[]): Set<number> {
  const hidden = new Set<number>();
  for (const part of shapes) {
    if (!part.combine) continue;
    hidden.add(part.combine.parent);
    for (const operation of part.combine.operations) {
      hidden.add(operation.shape);
    }
  }
  return hidden;
}

function drawCombinedShape(
  ctx: CanvasRenderingContext2D,
  meter: ShapeMeter,
  part: ShapePart,
  target: { x: number; y: number; width: number; height: number },
): void {
  const combine = part.combine;
  if (!combine) return;
  const sequence: { mode: string; shape: number }[] = [
    { mode: 'UNION', shape: combine.parent },
    ...combine.operations,
  ];
  const bounds = combineBounds(meter.shapes, sequence);
  if (bounds.width <= 0 || bounds.height <= 0) return;

  const offscreen = buildCombineMask(meter, combine, bounds, new Set());
  if (!offscreen) {
    drawShapePart(ctx, meter.shapes[combine.parent], meter, target);
    for (const operation of combine.operations) {
      const other = meter.shapes[operation.shape];
      if (other) drawShapePart(ctx, other, meter, target);
    }
    return;
  }

  const local = {
    x: -bounds.x,
    y: -bounds.y,
    width: bounds.width,
    height: bounds.height,
  };

  const fill = part.fill_color ?? meter.fill_color;
  const fill2 = part.fill_color2 ?? meter.fill_color2;
  offscreen.globalCompositeOperation = 'source-in';
  offscreen.fillStyle = shapeFill(
    offscreen,
    fill,
    fill2,
    meter.gradient_angle,
    { x: 0, y: 0, width: bounds.width, height: bounds.height },
  );
  offscreen.fillRect(0, 0, bounds.width, bounds.height);

  const stroke = part.stroke_color ?? meter.stroke_color;
  const stroke2 = part.stroke_color2 ?? meter.stroke_color2;
  const width = part.stroke_width ?? meter.stroke_width;
  const dash = part.stroke_dash ?? meter.stroke_dash;
  if (width > 0 && stroke.a > 0) {
    const parent = meter.shapes[combine.parent];
    offscreen.globalCompositeOperation = 'source-atop';
    offscreen.lineWidth = width;
    offscreen.strokeStyle = shapeStroke(
      offscreen,
      stroke,
      stroke2,
      meter.gradient_angle,
      { x: 0, y: 0, width: bounds.width, height: bounds.height },
    );
    if (dash.length) offscreen.setLineDash(dash);
    offscreen.save();
    offscreen.translate(local.x, local.y);
    applyShapeTransform(offscreen, parent);
    offscreen.beginPath();
    traceShape(offscreen, parent);
    offscreen.stroke();
    offscreen.restore();
    if (dash.length) offscreen.setLineDash([]);
  }

  ctx.drawImage(offscreen.canvas, bounds.x, bounds.y);
}

function combineOperation(mode: string): GlobalCompositeOperation {
  switch (mode.toUpperCase()) {
    case 'XOR':
      return 'xor';
    case 'INTERSECT':
      return 'destination-in';
    case 'EXCLUDE':
      return 'destination-out';
    default:
      return 'source-over';
  }
}

function buildCombineMask(
  meter: ShapeMeter,
  combine: ShapeCombine,
  bounds: { x: number; y: number; width: number; height: number },
  visited: Set<number>,
): CanvasRenderingContext2D | null {
  const offscreen = createOffscreenCanvas(
    Math.ceil(bounds.width),
    Math.ceil(bounds.height),
  );
  if (!offscreen) return null;

  const sequence = [
    { mode: 'UNION', shape: combine.parent },
    ...combine.operations,
  ];
  offscreen.clearRect(0, 0, bounds.width, bounds.height);
  for (let index = 0; index < sequence.length; index += 1) {
    const source = meter.shapes[sequence[index].shape];
    if (!source) continue;
    offscreen.save();
    offscreen.translate(-bounds.x, -bounds.y);
    offscreen.globalCompositeOperation =
      index === 0 ? 'source-over' : combineOperation(sequence[index].mode);
    if (source.combine) {
      const childBounds = combineBounds(
        meter.shapes,
        [
          { mode: 'UNION', shape: source.combine.parent },
          ...source.combine.operations,
        ],
        visited,
      );
      if (
        childBounds.width > 0 &&
        childBounds.height > 0 &&
        !visited.has(sequence[index].shape)
      ) {
        const child = buildCombineMask(
          meter,
          source.combine,
          childBounds,
          new Set([...visited, sequence[index].shape]),
        );
        if (child) {
          offscreen.drawImage(child.canvas, childBounds.x, childBounds.y);
        }
      }
    } else {
      drawShapePart(
        offscreen,
        source,
        meter,
        {
          x: -bounds.x,
          y: -bounds.y,
          width: bounds.width,
          height: bounds.height,
        },
        true,
      );
    }
    offscreen.restore();
  }
  return offscreen;
}

function combineBounds(
  shapes: ShapePart[],
  sequence: { mode: string; shape: number }[],
  visited: Set<number> = new Set(),
): { x: number; y: number; width: number; height: number } {
  let minX = Number.POSITIVE_INFINITY;
  let minY = Number.POSITIVE_INFINITY;
  let maxX = Number.NEGATIVE_INFINITY;
  let maxY = Number.NEGATIVE_INFINITY;
  for (const operation of sequence) {
    const part = shapes[operation.shape];
    if (!part) continue;
    let bounds = shapeBounds(part);
    if (part.combine && !visited.has(operation.shape)) {
      bounds = combineBounds(
        shapes,
        [
          { mode: 'UNION', shape: part.combine.parent },
          ...part.combine.operations,
        ],
        new Set([...visited, operation.shape]),
      );
    }
    minX = Math.min(minX, bounds.x);
    minY = Math.min(minY, bounds.y);
    maxX = Math.max(maxX, bounds.x + bounds.width);
    maxY = Math.max(maxY, bounds.y + bounds.height);
  }
  if (!Number.isFinite(minX)) return { x: 0, y: 0, width: 0, height: 0 };
  const pad = 2;
  return {
    x: minX - pad,
    y: minY - pad,
    width: Math.max(1, maxX - minX + pad * 2),
    height: Math.max(1, maxY - minY + pad * 2),
  };
}

function shapeBounds(part: ShapePart): {
  x: number;
  y: number;
  width: number;
  height: number;
} {
  let x = 0;
  let y = 0;
  let width = 0;
  let height = 0;
  if (part.kind === 'rectangle') {
    const [px = 0, py = 0, pw = 0, ph = 0] = part.params;
    x = Math.min(px, px + pw);
    y = Math.min(py, py + ph);
    width = Math.abs(pw);
    height = Math.abs(ph);
  } else if (part.kind === 'ellipse') {
    const [cx = 0, cy = 0, rx = 0, ry = 0] = part.params;
    x = cx - Math.abs(rx);
    y = cy - Math.abs(ry);
    width = Math.abs(rx) * 2;
    height = Math.abs(ry) * 2;
  } else if (part.kind === 'line') {
    const [x1 = 0, y1 = 0, x2 = 0, y2 = 0] = part.params;
    x = Math.min(x1, x2);
    y = Math.min(y1, y2);
    width = Math.abs(x2 - x1);
    height = Math.abs(y2 - y1);
  } else if (part.kind === 'curve') {
    const [x1 = 0, y1 = 0, x2 = 0, y2 = 0, cx1 = 0, cy1 = 0] = part.params;
    const xs = [x1, x2, cx1];
    const ys = [y1, y2, cy1];
    if (part.params.length >= 8) {
      xs.push(part.params[6]);
      ys.push(part.params[7]);
    }
    x = Math.min(...xs);
    y = Math.min(...ys);
    width = Math.max(...xs) - x;
    height = Math.max(...ys) - y;
  } else if (part.kind === 'arc') {
    const [x1 = 0, y1 = 0, x2 = 0, y2 = 0] = part.params;
    x = Math.min(x1, x2) - Math.abs(part.params[4] ?? 0);
    y = Math.min(y1, y2) - Math.abs(part.params[5] ?? 0);
    width = Math.abs(x2 - x1) + Math.abs(part.params[4] ?? 0) * 2;
    height = Math.abs(y2 - y1) + Math.abs(part.params[5] ?? 0) * 2;
  } else if (part.kind === 'path') {
    width = 100;
    height = 100;
  }
  return { x, y, width, height };
}

function applyShapeTransform(
  ctx: CanvasRenderingContext2D,
  part: ShapePart,
): void {
  if (part.offset) ctx.translate(part.offset[0], part.offset[1]);
  const rotate = part.rotate ?? 0;
  const scale = part.scale;
  const skew = part.skew;
  if (!rotate && !scale && !skew) return;

  const bounds = shapeBounds(part);
  const anchorX = part.anchor?.[0] ?? bounds.x + bounds.width / 2;
  const anchorY = part.anchor?.[1] ?? bounds.y + bounds.height / 2;
  ctx.translate(anchorX, anchorY);
  if (skew) {
    ctx.transform(
      1,
      Math.tan((skew[1] * Math.PI) / 180),
      Math.tan((skew[0] * Math.PI) / 180),
      1,
      0,
      0,
    );
  }
  if (scale) ctx.scale(scale[0], scale[1]);
  if (rotate) ctx.rotate((rotate * Math.PI) / 180);
  ctx.translate(-anchorX, -anchorY);
}

function createOffscreenCanvas(
  width: number,
  height: number,
): CanvasRenderingContext2D | null {
  if (typeof document === 'undefined') return null;
  try {
    const canvas = document.createElement('canvas');
    canvas.width = Math.max(1, width);
    canvas.height = Math.max(1, height);
    return canvas.getContext('2d');
  } catch {
    return null;
  }
}

function shapeFill(
  ctx: CanvasRenderingContext2D,
  color: Color,
  color2: Color | null,
  angle: number,
  rect: { x: number; y: number; width: number; height: number },
): string | CanvasGradient {
  return shapePaint(ctx, color, color2, angle, rect);
}

function shapeStroke(
  ctx: CanvasRenderingContext2D,
  color: Color,
  color2: Color | null,
  angle: number,
  rect: { x: number; y: number; width: number; height: number },
): string | CanvasGradient {
  return shapePaint(ctx, color, color2, angle, rect);
}

function shapePaint(
  ctx: CanvasRenderingContext2D,
  color: Color,
  color2: Color | null,
  angle: number,
  rect: { x: number; y: number; width: number; height: number },
): string | CanvasGradient {
  if (!color2) return cssColor(color);
  const radians = ((angle - 90) * Math.PI) / 180;
  const cx = rect.width / 2;
  const cy = rect.height / 2;
  const length =
    (Math.abs(rect.width * Math.cos(radians)) +
      Math.abs(rect.height * Math.sin(radians))) /
    2;
  const gradient = ctx.createLinearGradient(
    cx - Math.cos(radians) * length,
    cy - Math.sin(radians) * length,
    cx + Math.cos(radians) * length,
    cy + Math.sin(radians) * length,
  );
  gradient.addColorStop(0, cssColor(color));
  gradient.addColorStop(1, cssColor(color2));
  return gradient;
}

function traceShape(ctx: CanvasRenderingContext2D, part: ShapePart): void {
  if (part.kind === 'rectangle') {
    const [x = 0, y = 0, width = 0, height = 0, radius = 0] = part.params;
    if (radius > 0) {
      const r = Math.min(radius, width / 2, height / 2);
      ctx.moveTo(x + r, y);
      ctx.lineTo(x + width - r, y);
      ctx.quadraticCurveTo(x + width, y, x + width, y + r);
      ctx.lineTo(x + width, y + height - r);
      ctx.quadraticCurveTo(x + width, y + height, x + width - r, y + height);
      ctx.lineTo(x + r, y + height);
      ctx.quadraticCurveTo(x, y + height, x, y + height - r);
      ctx.lineTo(x, y + r);
      ctx.quadraticCurveTo(x, y, x + r, y);
    } else {
      ctx.rect(x, y, width, height);
    }
    return;
  }

  if (part.kind === 'ellipse') {
    const [cx = 0, cy = 0, rx = 0, ry = 0] = part.params;
    ctx.ellipse(cx, cy, Math.abs(rx), Math.abs(ry), 0, 0, Math.PI * 2);
    return;
  }

  if (part.kind === 'line') {
    const [x1 = 0, y1 = 0, x2 = 0, y2 = 0] = part.params;
    ctx.moveTo(x1, y1);
    ctx.lineTo(x2, y2);
    return;
  }

  if (part.kind === 'curve') {
    const [x1 = 0, y1 = 0, x2 = 0, y2 = 0, cx1 = 0, cy1 = 0] = part.params;
    ctx.moveTo(x1, y1);
    if (part.params.length >= 8) {
      const [, , , , , , cx2 = 0, cy2 = 0] = part.params;
      ctx.bezierCurveTo(cx1, cy1, cx2, cy2, x2, y2);
    } else {
      ctx.quadraticCurveTo(cx1, cy1, x2, y2);
    }
    return;
  }

  if (part.kind === 'arc') {
    traceArc(ctx, part.params);
    return;
  }

  if (part.kind === 'path' && part.path) {
    tracePath(ctx, part.path);
  }
}

function traceArc(ctx: CanvasRenderingContext2D, params: number[]): void {
  const [x1 = 0, y1 = 0, x2 = 0, y2 = 0] = params;
  const dx = x2 - x1;
  const dy = y2 - y1;
  const distance = Math.hypot(dx, dy);
  if (distance === 0) {
    ctx.moveTo(x1, y1);
    return;
  }

  const rotation = ((params[6] ?? 0) * Math.PI) / 180;
  let rx = Math.abs(params[4] ?? distance / 2);
  let ry = Math.abs(params[5] ?? rx);
  rx = Math.max(rx, distance / 2);
  ry = Math.max(ry, distance / 2);

  const cos = Math.cos(rotation);
  const sin = Math.sin(rotation);
  const halfX = (x1 - x2) / 2;
  const halfY = (y1 - y2) / 2;
  const localX = cos * halfX + sin * halfY;
  const localY = -sin * halfX + cos * halfY;
  const lambda = (localX * localX) / (rx * rx) + (localY * localY) / (ry * ry);
  if (lambda > 1) {
    const scale = Math.sqrt(lambda);
    rx *= scale;
    ry *= scale;
  }

  const numerator = Math.max(
    0,
    rx * rx * ry * ry - rx * rx * localY * localY - ry * ry * localX * localX,
  );
  const denominator = rx * rx * localY * localY + ry * ry * localX * localX;
  let factor = denominator === 0 ? 0 : Math.sqrt(numerator / denominator);
  const sweep = params[7] === undefined ? true : params[7] !== 0;
  const small = params[8] === undefined ? false : params[8] !== 0;
  if (sweep === small) factor = -factor;

  const centerX =
    (cos * factor * (rx * localY)) / ry -
    (sin * factor * (ry * localX)) / rx +
    (x1 + x2) / 2;
  const centerY =
    (sin * factor * (rx * localY)) / ry +
    (cos * factor * (ry * localX)) / rx +
    (y1 + y2) / 2;

  const start = Math.atan2(
    (localY - (factor * (rx * localY)) / ry) / ry,
    (localX - (factor * (ry * localX)) / rx) / rx,
  );
  const end = Math.atan2(
    (-localY - (factor * (rx * localY)) / ry) / ry,
    (-localX - (factor * (ry * localX)) / rx) / rx,
  );

  const counterClockwise = !sweep;
  ctx.ellipse(centerX, centerY, rx, ry, rotation, start, end, counterClockwise);
}

function isOpenShape(part: ShapePart): boolean {
  if (part.kind === 'line') return true;
  if (part.kind === 'arc') {
    return part.params.length <= 9 || part.params[9] === 0;
  }
  if (part.kind === 'curve') {
    if (part.params.length >= 9) return part.params[8] === 0;
    if (part.params.length === 7) return part.params[6] === 0;
    return true;
  }
  return false;
}

function tracePath(ctx: CanvasRenderingContext2D, path: string): void {
  const tokens = path.match(/[a-zA-Z]|-?\d*\.?\d+(?:e[-+]?\d+)?/gi) ?? [];
  let index = 0;
  let command = '';
  let x = 0;
  let y = 0;

  const number = (): number => {
    const value = Number(tokens[index] ?? 0);
    index += 1;
    return Number.isFinite(value) ? value : 0;
  };

  while (index < tokens.length) {
    if (/^[a-zA-Z]$/.test(tokens[index])) {
      command = tokens[index];
      index += 1;
    }
    const relative = command === command.toLowerCase();
    const upper = command.toUpperCase();
    if (upper === 'M' || upper === 'L') {
      const nx = number() + (relative ? x : 0);
      const ny = number() + (relative ? y : 0);
      if (upper === 'M') ctx.moveTo(nx, ny);
      else ctx.lineTo(nx, ny);
      x = nx;
      y = ny;
    } else if (upper === 'C') {
      const x1 = number() + (relative ? x : 0);
      const y1 = number() + (relative ? y : 0);
      const x2 = number() + (relative ? x : 0);
      const y2 = number() + (relative ? y : 0);
      const nx = number() + (relative ? x : 0);
      const ny = number() + (relative ? y : 0);
      ctx.bezierCurveTo(x1, y1, x2, y2, nx, ny);
      x = nx;
      y = ny;
    } else if (upper === 'Q') {
      const x1 = number() + (relative ? x : 0);
      const y1 = number() + (relative ? y : 0);
      const nx = number() + (relative ? x : 0);
      const ny = number() + (relative ? y : 0);
      ctx.quadraticCurveTo(x1, y1, nx, ny);
      x = nx;
      y = ny;
    } else if (upper === 'Z') {
      ctx.closePath();
      return;
    } else {
      index += 1;
    }
  }
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

interface Rect {
  x: number;
  y: number;
  width: number;
  height: number;
}

function drawHistogram(
  ctx: CanvasRenderingContext2D,
  box: MeterBox,
  getImage: ImageSourceFn,
  histories: MeterHistories,
): void {
  const meter = box.meter as HistogramMeter;
  const target = box.content;
  if (target.width <= 0 || target.height <= 0) return;

  const vertical = meter.graph_orientation !== 'HORIZONTAL';
  const capacity = Math.max(
    1,
    Math.floor(vertical ? target.width : target.height),
  );
  const history = histories.get(meter.name) ?? [
    [meter.primary],
    [meter.secondary],
  ];
  const primary = history[0] ?? [];
  const secondary = history[1] ?? [];
  const visible = Math.min(
    Math.max(primary.length, secondary.length),
    capacity,
  );
  if (visible <= 0) return;

  const primaryBase = primary.length - visible;
  const secondaryBase = secondary.length - visible;
  let scale = 1;
  if (meter.auto_scale) {
    scale = 0;
    for (let index = 0; index < visible; index += 1) {
      scale = Math.max(
        scale,
        primary[primaryBase + index] ?? 0,
        secondary[secondaryBase + index] ?? 0,
      );
    }
    if (scale <= 0) scale = 1;
  }

  const cell = (vertical ? target.width : target.height) / capacity;
  const size = Math.max(1, cell);

  for (let index = 0; index < visible; index += 1) {
    const primaryValue = clamp((primary[primaryBase + index] ?? 0) / scale);
    const secondaryValue = clamp(
      (secondary[secondaryBase + index] ?? 0) / scale,
    );
    const column = graphPosition(index, visible, capacity, meter.graph_start);

    if (vertical) {
      const x = target.x + column * cell;
      const primaryHeight = primaryValue * target.height;
      const secondaryHeight = secondaryValue * target.height;
      const primaryTop = meter.flip ? 0 : target.height - primaryHeight;
      const secondaryTop = meter.flip ? target.height - secondaryHeight : 0;
      const overlap = Math.max(
        0,
        primaryHeight + secondaryHeight - target.height,
      );
      const bothTop = meter.flip
        ? target.height - secondaryHeight
        : target.height - primaryHeight;

      paintRect(
        ctx,
        { x, y: target.y + primaryTop, width: size, height: primaryHeight },
        meter.primary_color,
        meter.primary_image,
        getImage,
      );
      paintRect(
        ctx,
        {
          x,
          y: target.y + secondaryTop,
          width: size,
          height: secondaryHeight,
        },
        meter.secondary_color,
        meter.secondary_image,
        getImage,
      );
      if (overlap > 0) {
        paintRect(
          ctx,
          { x, y: target.y + bothTop, width: size, height: overlap },
          meter.both_color,
          meter.both_image,
          getImage,
        );
      }
    } else {
      const y = target.y + column * cell;
      const primaryWidth = primaryValue * target.width;
      const secondaryWidth = secondaryValue * target.width;
      const primaryLeft = meter.flip ? 0 : target.width - primaryWidth;
      const secondaryLeft = meter.flip ? target.width - secondaryWidth : 0;
      const overlap = Math.max(0, primaryWidth + secondaryWidth - target.width);
      const bothLeft = meter.flip
        ? target.width - secondaryWidth
        : target.width - primaryWidth;

      paintRect(
        ctx,
        { x: target.x + primaryLeft, y, width: primaryWidth, height: size },
        meter.primary_color,
        meter.primary_image,
        getImage,
      );
      paintRect(
        ctx,
        {
          x: target.x + secondaryLeft,
          y,
          width: secondaryWidth,
          height: size,
        },
        meter.secondary_color,
        meter.secondary_image,
        getImage,
      );
      if (overlap > 0) {
        paintRect(
          ctx,
          { x: target.x + bothLeft, y, width: overlap, height: size },
          meter.both_color,
          meter.both_image,
          getImage,
        );
      }
    }
  }
}

function drawLine(
  ctx: CanvasRenderingContext2D,
  box: MeterBox,
  histories: MeterHistories,
): void {
  const meter = box.meter as LineMeter;
  const target = box.content;
  if (target.width <= 0 || target.height <= 0) return;

  const vertical = meter.graph_orientation === 'VERTICAL';
  const capacity = Math.max(
    2,
    Math.floor(vertical ? target.height : target.width),
  );
  const span = vertical ? target.height : target.width;
  const step = span / (capacity - 1);
  const history =
    histories.get(meter.name) ?? meter.values.map((value) => [value]);

  if (meter.horizontal_lines > 0) {
    ctx.strokeStyle = cssColor(meter.horizontal_color);
    ctx.lineWidth = 1;
    for (let index = 1; index <= meter.horizontal_lines; index += 1) {
      const fraction = index / (meter.horizontal_lines + 1);
      ctx.beginPath();
      if (vertical) {
        const x = target.x + target.width * fraction;
        ctx.moveTo(x, target.y);
        ctx.lineTo(x, target.y + target.height);
      } else {
        const y = target.y + target.height * fraction;
        ctx.moveTo(target.x, y);
        ctx.lineTo(target.x + target.width, y);
      }
      ctx.stroke();
    }
  }

  for (let line = 0; line < meter.line_count; line += 1) {
    const channel = history[line] ?? [];
    const visible = Math.min(channel.length, capacity);
    if (visible <= 0) continue;

    const base = channel.length - visible;
    let scale = 1;
    if (meter.auto_scale) {
      scale = 0;
      for (let index = 0; index < visible; index += 1) {
        scale = Math.max(scale, channel[base + index] ?? 0);
      }
      if (scale <= 0) scale = 1;
    }
    const factor = meter.scales[line] ?? 1;
    const color = meter.colors[line] ??
      meter.colors.at(-1) ?? {
        r: 255,
        g: 255,
        b: 255,
        a: 255,
      };
    const width = Math.max(1, meter.line_width);

    ctx.strokeStyle = cssColor(color);
    ctx.fillStyle = cssColor(color);
    ctx.lineWidth = width;

    const points: { x: number; y: number }[] = [];
    for (let index = 0; index < visible; index += 1) {
      const value = clamp(((channel[base + index] ?? 0) * factor) / scale);
      const position = graphPosition(
        index,
        visible,
        capacity,
        meter.graph_start,
      );
      if (vertical) {
        points.push({
          x: target.x + (meter.flip ? value : 1 - value) * target.width,
          y: target.y + position * step,
        });
      } else {
        points.push({
          x: target.x + position * step,
          y: target.y + (meter.flip ? value : 1 - value) * target.height,
        });
      }
    }

    if (points.length === 1) {
      const point = points[0];
      ctx.fillRect(point.x - width / 2, point.y - width / 2, width, width);
      continue;
    }

    ctx.beginPath();
    ctx.moveTo(points[0].x, points[0].y);
    for (let index = 1; index < points.length; index += 1) {
      ctx.lineTo(points[index].x, points[index].y);
    }
    ctx.stroke();
  }
}

function drawRotator(
  ctx: CanvasRenderingContext2D,
  box: MeterBox,
  getImage: ImageSourceFn,
): void {
  const meter = box.meter as RotatorMeter;
  if (!meter.image) return;
  const image = getImage(meter.image);
  if (!image) return;
  const width = imageWidth(image);
  const height = imageHeight(image);
  if (width <= 0 || height <= 0) return;

  const centerX = box.content.x + box.content.width / 2 + meter.offset_x;
  const centerY = box.content.y + box.content.height / 2 + meter.offset_y;
  const value =
    meter.value_remainder > 0
      ? (meter.value % meter.value_remainder) / meter.value_remainder
      : meter.value;
  const angle = meter.start_angle + meter.rotation_angle * value;

  ctx.save();
  ctx.translate(centerX, centerY);
  ctx.rotate(angle);
  ctx.drawImage(image, -width / 2, -height / 2, width, height);
  ctx.restore();
}

function drawRoundLine(ctx: CanvasRenderingContext2D, box: MeterBox): void {
  const meter = box.meter as RoundLineMeter;
  const target = box.content;
  if (target.width <= 0 || target.height <= 0) return;

  const centerX = target.x + target.width / 2;
  const centerY = target.y + target.height / 2;
  const radius =
    meter.line_length > 0
      ? meter.line_length
      : Math.max(
          0,
          Math.min(target.width, target.height) / 2 - meter.line_width / 2,
        );
  if (radius <= 0) return;

  const remainder =
    meter.value_remainder > 0
      ? (meter.value % meter.value_remainder) / meter.value_remainder
      : meter.value;
  const start =
    meter.start_angle + (meter.control_start ? meter.start_shift : 0);
  const length =
    meter.rotation_angle * remainder +
    (meter.control_length ? meter.length_shift : 0);

  ctx.strokeStyle = cssColor(meter.line_color);
  ctx.lineWidth = Math.max(0.5, meter.line_width);
  ctx.beginPath();
  ctx.arc(centerX, centerY, radius, start, start + length, false);
  ctx.stroke();
}

function drawBitmap(
  ctx: CanvasRenderingContext2D,
  box: MeterBox,
  getImage: ImageSourceFn,
  state: RenderState,
): void {
  const meter = box.meter as BitmapMeter;
  if (!meter.image) return;
  const image = getImage(meter.image);
  if (!image) return;
  const imageW = imageWidth(image);
  const imageH = imageHeight(image);
  const totalFrames = Math.max(1, meter.frames);
  const transitionFrames = Math.max(0, meter.transition_frames);
  const realFrames = Math.max(
    1,
    Math.floor(totalFrames / (transitionFrames + 1)),
  );
  const separation = meter.separation;
  const frameW =
    (imageW - separation * Math.max(0, totalFrames - 1)) / totalFrames;
  if (frameW <= 0 || imageH <= 0) return;

  const target = box.content;
  if (meter.digits > 0) {
    const text = meter.text || '0';
    const totalWidth = frameW * text.length;
    const startX =
      meter.align === 'CENTER'
        ? target.x + (target.width - totalWidth) / 2
        : meter.align === 'RIGHT'
          ? target.x + target.width - totalWidth
          : target.x;
    for (let index = 0; index < text.length; index += 1) {
      const character = text[index];
      let frame: number;
      if (character >= '0' && character <= '9') {
        frame = Number(character) + (meter.zero_frame ? 0 : 1);
      } else if (character === '-') {
        frame = realFrames - 1;
      } else {
        continue;
      }
      if (frame < 0 || frame >= realFrames) continue;
      ctx.drawImage(
        image,
        frame * (frameW + separation),
        0,
        frameW,
        imageH,
        startX + index * frameW,
        target.y,
        frameW,
        target.height || imageH,
      );
    }
    return;
  }

  const transition = updateBitmapTransition(
    meter,
    realFrames,
    transitionFrames,
    state,
  );
  const frame = Math.min(
    totalFrames - 1,
    Math.max(0, transition.frame * (transitionFrames + 1) + transition.step),
  );
  ctx.drawImage(
    image,
    frame * (frameW + separation),
    0,
    frameW,
    imageH,
    target.x,
    target.y,
    target.width || frameW,
    target.height || imageH,
  );
  if (meter.extend && frame >= totalFrames - 1) {
    ctx.drawImage(
      image,
      frame * (frameW + separation),
      0,
      frameW,
      imageH,
      target.x + frameW,
      target.y,
      Math.max(0, target.width - frameW),
      target.height || imageH,
    );
  }
}

function updateBitmapTransition(
  meter: BitmapMeter,
  realFrames: number,
  transitionFrames: number,
  state: RenderState,
): { frame: number; step: number } {
  const value = Math.min(Math.max(meter.value, 0), 1);
  const targetFrame = Math.max(
    0,
    Math.min(realFrames - 1, Math.round(value * (realFrames - 1))),
  );
  if (transitionFrames <= 0 || !state.transitions || state.now === undefined) {
    return { frame: targetFrame, step: 0 };
  }

  const key = meter.name;
  const now = state.now;
  let transition = state.transitions.get(key);
  if (!transition || transition.value !== value) {
    transition = {
      value,
      startValue: transition?.value ?? value,
      startTime: now,
    };
    state.transitions.set(key, transition);
  }

  const duration = transitionFrames * 80;
  const elapsed = now - transition.startTime;
  if (transition.startValue === value || elapsed >= duration) {
    return { frame: targetFrame, step: 0 };
  }

  const progress = Math.min(1, Math.max(0, elapsed / duration));
  const interpolated =
    transition.startValue + (value - transition.startValue) * progress;
  const frame = Math.max(
    0,
    Math.min(realFrames - 1, Math.round(interpolated * (realFrames - 1))),
  );
  const step = Math.min(
    transitionFrames - 1,
    Math.max(0, Math.floor(progress * transitionFrames)),
  );
  return { frame, step };
}

function drawButton(
  ctx: CanvasRenderingContext2D,
  box: MeterBox,
  getImage: ImageSourceFn,
  state: RenderState,
): void {
  const meter = box.meter as ButtonMeter;
  if (!meter.image) return;
  const image = getImage(meter.image);
  if (!image) return;
  const imageW = imageWidth(image);
  const imageH = imageHeight(image);
  const frameW = imageW / 3;
  if (frameW <= 0 || imageH <= 0) return;

  const frame =
    state.pressed === meter.name ? 1 : state.hovered === meter.name ? 2 : 0;
  ctx.drawImage(
    image,
    frame * frameW,
    0,
    frameW,
    imageH,
    box.content.x,
    box.content.y,
    box.content.width || frameW,
    box.content.height || imageH,
  );
}

function graphPosition(
  index: number,
  visible: number,
  capacity: number,
  graphStart: string,
): number {
  if (graphStart.toUpperCase() === 'LEFT') {
    return visible - 1 - index;
  }
  return capacity - visible + index;
}

function paintRect(
  ctx: CanvasRenderingContext2D,
  rect: Rect,
  color: Color,
  imagePath: string | null,
  getImage: ImageSourceFn,
): void {
  if (rect.width <= 0 || rect.height <= 0) return;
  if (imagePath) {
    const image = getImage(imagePath);
    if (image) {
      ctx.save();
      ctx.beginPath();
      ctx.rect(rect.x, rect.y, rect.width, rect.height);
      ctx.clip();
      ctx.drawImage(image, rect.x, rect.y, rect.width, rect.height);
      ctx.restore();
      return;
    }
  }
  if (color.a <= 0) return;
  ctx.fillStyle = cssColor(color);
  ctx.fillRect(rect.x, rect.y, rect.width, rect.height);
}

function clamp(value: number): number {
  return Math.min(Math.max(value, 0), 1);
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

export function fontString(meter: StringMeter, style?: TextStyle): string {
  const resolved = style ?? baseStyle(meter);
  const italic = resolved.italic ? 'italic ' : '';
  const weight = resolved.weight > 0 ? `${resolved.weight}` : 'normal';
  return `${italic}${weight} ${resolved.size}px "${resolved.face}", sans-serif`;
}

function imageWidth(image: CanvasImageSource): number {
  const element = image as HTMLImageElement;
  return element.naturalWidth || element.width || 0;
}

function imageHeight(image: CanvasImageSource): number {
  const element = image as HTMLImageElement;
  return element.naturalHeight || element.height || 0;
}
