import { buildTextRuns, TextRun, TextStyle } from './inline';
import {
  BitmapMeter,
  ButtonMeter,
  Dimension,
  ImageMeter,
  RenderMeter,
  RenderModel,
  RotatorMeter,
  RoundLineMeter,
  ShapeMeter,
  StringMeter,
} from './skin-model';

export type SizedMeter = ImageMeter | RotatorMeter | BitmapMeter | ButtonMeter;

export interface TextMeasurement {
  width: number;
  height: number;
}

export type MeasureTextFn = (
  meter: StringMeter,
  text: string,
  style?: TextStyle,
) => TextMeasurement;

export type ImageSizeFn = (
  meter: SizedMeter,
) => { width: number; height: number } | null;

export interface PlacedRun extends TextRun {
  x: number;
  width: number;
}

export interface TextPlacement {
  value: string;
  x: number;
  y: number;
  align: 'left' | 'center' | 'right';
  baseline: 'top' | 'middle' | 'bottom';
  runs: PlacedRun[];
}

export interface MeterBox {
  meter: RenderMeter;
  x: number;
  y: number;
  width: number;
  height: number;
  content: { x: number; y: number; width: number; height: number };
  text: TextPlacement | null;
  clip: { x: number; y: number; width: number; height: number } | null;
}

export interface SkinLayout {
  width: number;
  height: number;
  boxes: MeterBox[];
}

const DEFAULT_BAR_WIDTH = 100;
const DEFAULT_BAR_HEIGHT = 10;
const DEFAULT_GRAPH_WIDTH = 100;
const DEFAULT_GRAPH_HEIGHT = 40;

export function lookup(values: Record<string, string>, name: string): string {
  const key = Object.keys(values).find(
    (candidate) => candidate.toLowerCase() === name.toLowerCase(),
  );
  return key ? values[key] : '';
}

export function resolveText(
  meter: StringMeter,
  values: Record<string, string>,
): string {
  let base: string;
  if (meter.text === null || meter.text === undefined) {
    base = meter.measure_names.length
      ? lookup(values, meter.measure_names[0])
      : '';
  } else {
    base = meter.text.replace(/%(\d)/g, (match, digit: string) => {
      const index = Number(digit) - 1;
      const name = meter.measure_names[index];
      return name ? lookup(values, name) : match;
    });
  }
  base = applyCase(base, meter.string_case);
  return `${meter.prefix}${base}${meter.postfix}`;
}

function applyCase(value: string, mode: string): string {
  switch (mode.toUpperCase()) {
    case 'UPPER':
      return value.toUpperCase();
    case 'LOWER':
      return value.toLowerCase();
    case 'PROPER':
      return value.replace(/\b\w/g, (c) => c.toUpperCase());
    default:
      return value;
  }
}

function resolveAxis(
  dimension: Dimension,
  previousStart: number,
  previousSize: number,
): number {
  switch (dimension.relative) {
    case 'position':
      return previousStart + dimension.value;
    case 'edge':
      return previousStart + previousSize + dimension.value;
    default:
      return dimension.value;
  }
}

function shapeExtent(meter: ShapeMeter): { width: number; height: number } {
  let width = 0;
  let height = 0;
  for (const part of meter.shapes) {
    const [a = 0, b = 0, c = 0, d = 0] = part.params;
    if (part.kind === 'rectangle') {
      width = Math.max(width, a + c);
      height = Math.max(height, b + d);
    } else if (part.kind === 'ellipse') {
      width = Math.max(width, a + c);
      height = Math.max(height, b + d);
    } else if (part.kind === 'line') {
      width = Math.max(width, a, c);
      height = Math.max(height, b, d);
    } else if (part.kind === 'curve') {
      const xs = [a, c, part.params[4] ?? 0];
      const ys = [b, d, part.params[5] ?? 0];
      if (part.params.length >= 8) {
        xs.push(part.params[6]);
        ys.push(part.params[7]);
      }
      width = Math.max(width, ...xs);
      height = Math.max(height, ...ys);
    } else if (part.kind === 'arc') {
      width = Math.max(
        width,
        a + Math.abs(part.params[4] ?? 0),
        c + Math.abs(part.params[4] ?? 0),
      );
      height = Math.max(
        height,
        b + Math.abs(part.params[5] ?? 0),
        d + Math.abs(part.params[5] ?? 0),
      );
    } else if (part.kind === 'path') {
      width = Math.max(width, 100);
      height = Math.max(height, 100);
    }
  }
  return { width, height };
}

function horizontalAlign(value: string): 'left' | 'center' | 'right' {
  const upper = value.toUpperCase();
  if (upper.includes('RIGHT')) return 'right';
  if (upper.includes('CENTER')) return 'center';
  return 'left';
}

function verticalBaseline(value: string): 'top' | 'middle' | 'bottom' {
  const upper = value.toUpperCase();
  if (upper.includes('BOTTOM')) return 'bottom';
  if (upper.includes('CENTER') || upper.includes('MIDDLE')) return 'middle';
  return 'top';
}

export type AlphaHitTest = (box: MeterBox, x: number, y: number) => boolean;

export function hitTest(
  boxes: MeterBox[],
  x: number,
  y: number,
  alphaHitTest?: AlphaHitTest,
): MeterBox | null {
  for (let index = boxes.length - 1; index >= 0; index -= 1) {
    const box = boxes[index];
    if (box.width <= 0 || box.height <= 0) continue;
    if (box.clip && (box.clip.width <= 0 || box.clip.height <= 0)) continue;
    if (
      x >= box.x &&
      x <= box.x + box.width &&
      y >= box.y &&
      y <= box.y + box.height
    ) {
      if (
        box.clip &&
        (x < box.clip.x ||
          x > box.clip.x + box.clip.width ||
          y < box.clip.y ||
          y > box.clip.y + box.clip.height)
      ) {
        continue;
      }
      if (alphaHitTest && !alphaHitTest(box, x, y)) continue;
      return box;
    }
  }
  return null;
}

export function layoutSkin(
  model: RenderModel,
  measureText: MeasureTextFn,
  imageSize: ImageSizeFn = () => null,
): SkinLayout {
  const boxes: MeterBox[] = [];
  let previousX = 0;
  let previousY = 0;
  let previousWidth = 0;
  let previousHeight = 0;
  let maxRight = 0;
  let maxBottom = 0;

  for (const meter of model.meters) {
    const x = resolveAxis(meter.x, previousX, previousWidth);
    const y = resolveAxis(meter.y, previousY, previousHeight);
    const box = layoutMeter(
      meter,
      x,
      y,
      model.measure_values,
      measureText,
      imageSize,
    );
    boxes.push(box);
    previousX = box.x;
    previousY = box.y;
    previousWidth = box.width;
    previousHeight = box.height;
    maxRight = Math.max(maxRight, box.x + box.width);
    maxBottom = Math.max(maxBottom, box.y + box.height);
  }

  applyContainers(boxes);

  return {
    width: model.width ?? Math.ceil(maxRight),
    height: model.height ?? Math.ceil(maxBottom),
    boxes,
  };
}

function applyContainers(boxes: MeterBox[]): void {
  const byName = new Map(
    boxes.map((box) => [box.meter.name.toLowerCase(), box]),
  );
  const resolved = new Map<
    MeterBox,
    { x: number; y: number; width: number; height: number } | null
  >();

  const resolve = (
    box: MeterBox,
    visiting: Set<MeterBox>,
  ): { x: number; y: number; width: number; height: number } | null => {
    if (resolved.has(box)) return resolved.get(box) ?? null;
    const name = box.meter.container;
    if (!name) {
      resolved.set(box, null);
      return null;
    }
    if (visiting.has(box)) {
      const empty = { x: box.content.x, y: box.content.y, width: 0, height: 0 };
      resolved.set(box, empty);
      return empty;
    }

    const container = byName.get(name.toLowerCase());
    if (!container || container.width <= 0 || container.height <= 0) {
      const empty = { x: box.content.x, y: box.content.y, width: 0, height: 0 };
      resolved.set(box, empty);
      return empty;
    }

    visiting.add(box);
    const parentClip = resolve(container, visiting);
    visiting.delete(box);

    let clip = intersectRect(box.content, container.content);
    if (parentClip) clip = intersectRect(clip, parentClip);
    if (clip.width <= 0 || clip.height <= 0) {
      clip = { x: box.content.x, y: box.content.y, width: 0, height: 0 };
    }
    resolved.set(box, clip);
    return clip;
  };

  for (const box of boxes) {
    if (box.meter.container) {
      box.clip = resolve(box, new Set());
    }
  }
}

function intersectRect(
  first: { x: number; y: number; width: number; height: number },
  second: { x: number; y: number; width: number; height: number },
): { x: number; y: number; width: number; height: number } {
  const x = Math.max(first.x, second.x);
  const y = Math.max(first.y, second.y);
  const right = Math.min(first.x + first.width, second.x + second.width);
  const bottom = Math.min(first.y + first.height, second.y + second.height);
  return {
    x,
    y,
    width: Math.max(0, right - x),
    height: Math.max(0, bottom - y),
  };
}

function measureRun(
  meter: StringMeter,
  run: TextRun,
  measureText: MeasureTextFn,
): number {
  const spacing = run.style.spacing;
  if (!spacing) return measureText(meter, run.text, run.style).width;

  let width = 0;
  for (const character of Array.from(run.text)) {
    const advance =
      spacing.advance ?? measureText(meter, character, run.style).width;
    width += advance + spacing.leading + spacing.trailing;
  }
  return width;
}

function layoutMeter(
  meter: RenderMeter,
  x: number,
  y: number,
  values: Record<string, string>,
  measureText: MeasureTextFn,
  imageSize: ImageSizeFn,
): MeterBox {
  const [padLeft, padTop, padRight, padBottom] = meter.padding;
  const padWidth = padLeft + padRight;
  const padHeight = padTop + padBottom;

  if (meter.hidden) {
    return {
      meter,
      x,
      y,
      width: 0,
      height: 0,
      content: { x, y, width: 0, height: 0 },
      text: null,
      clip: null,
    };
  }

  let contentWidth = 0;
  let contentHeight = 0;
  let text: TextPlacement | null = null;

  if (meter.type === 'string') {
    const value = resolveText(meter, values);
    const runs = buildTextRuns(meter, value);
    const placed: PlacedRun[] = [];
    let width = 0;
    let height = 0;
    for (const run of runs) {
      const runWidth = measureRun(meter, run, measureText);
      const measured = measureText(meter, run.text, run.style);
      height = Math.max(height, measured.height);
      placed.push({ ...run, x: 0, width: runWidth });
      width += runWidth;
    }
    contentWidth = width || measureText(meter, value).width;
    contentHeight = height || measureText(meter, value).height;
    text = {
      value,
      x: 0,
      y: 0,
      align: 'left',
      baseline: 'top',
      runs: placed,
    };
  } else if (meter.type === 'image') {
    const size = imageSize(meter);
    contentWidth = size?.width ?? 0;
    contentHeight = size?.height ?? 0;
  } else if (meter.type === 'bar') {
    contentWidth = DEFAULT_BAR_WIDTH;
    contentHeight = DEFAULT_BAR_HEIGHT;
  } else if (meter.type === 'histogram') {
    contentWidth = DEFAULT_GRAPH_WIDTH;
    contentHeight = DEFAULT_GRAPH_HEIGHT;
  } else if (meter.type === 'line') {
    contentWidth = DEFAULT_GRAPH_WIDTH;
    contentHeight = DEFAULT_GRAPH_HEIGHT;
  } else if (meter.type === 'rotator' || meter.type === 'button') {
    const size = imageSize(meter);
    contentWidth =
      meter.type === 'button' ? (size?.width ?? 0) / 3 : (size?.width ?? 0);
    contentHeight = size?.height ?? 0;
  } else if (meter.type === 'bitmap') {
    const size = imageSize(meter);
    const frames = Math.max(1, meter.frames);
    const separation = meter.separation * Math.max(0, frames - 1);
    contentWidth = Math.max(0, ((size?.width ?? 0) - separation) / frames);
    contentHeight = size?.height ?? 0;
  } else if (meter.type === 'roundline') {
    const diameter = meter.line_length * 2 + meter.line_width;
    contentWidth = diameter;
    contentHeight = diameter;
  } else if (meter.type === 'shape') {
    const extent = shapeExtent(meter);
    contentWidth = extent.width;
    contentHeight = extent.height;
  }

  const width = meter.width ?? Math.max(0, contentWidth + padWidth);
  const height = meter.height ?? Math.max(0, contentHeight + padHeight);

  const content = {
    x: x + padLeft,
    y: y + padTop,
    width: Math.max(0, width - padWidth),
    height: Math.max(0, height - padHeight),
  };

  if (meter.type === 'string' && text) {
    const align = horizontalAlign(meter.string_align);
    const baseline = verticalBaseline(meter.string_align);
    text.align = align;
    text.baseline = baseline;
    text.x = content.x;
    if (align === 'center') text.x = content.x + content.width / 2;
    else if (align === 'right') text.x = content.x + content.width;
    text.y = content.y;
    if (baseline === 'middle') text.y = content.y + content.height / 2;
    else if (baseline === 'bottom') text.y = content.y + content.height;
    const total = text.runs.reduce((sum, run) => sum + run.width, 0);
    let offset =
      align === 'center'
        ? text.x - total / 2
        : align === 'right'
          ? text.x - total
          : text.x;
    for (const run of text.runs) {
      run.x = offset;
      offset += run.width;
    }
  }

  return {
    meter,
    x,
    y,
    width: Math.ceil(width),
    height: Math.ceil(height),
    content,
    text,
    clip: null,
  };
}
