import {
  Dimension,
  ImageMeter,
  RenderMeter,
  RenderModel,
  StringMeter,
} from './skin-model';

export interface TextMeasurement {
  width: number;
  height: number;
}

export type MeasureTextFn = (
  meter: StringMeter,
  text: string,
) => TextMeasurement;

export type ImageSizeFn = (
  meter: ImageMeter,
) => { width: number; height: number } | null;

export interface TextPlacement {
  value: string;
  x: number;
  y: number;
  align: 'left' | 'center' | 'right';
  baseline: 'top' | 'middle' | 'bottom';
}

export interface MeterBox {
  meter: RenderMeter;
  x: number;
  y: number;
  width: number;
  height: number;
  content: { x: number; y: number; width: number; height: number };
  text: TextPlacement | null;
}

export interface SkinLayout {
  width: number;
  height: number;
  boxes: MeterBox[];
}

const DEFAULT_BAR_WIDTH = 100;
const DEFAULT_BAR_HEIGHT = 10;

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

  return {
    width: model.width ?? Math.ceil(maxRight),
    height: model.height ?? Math.ceil(maxBottom),
    boxes,
  };
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
    };
  }

  let contentWidth = 0;
  let contentHeight = 0;
  let text: TextPlacement | null = null;

  if (meter.type === 'string') {
    const value = resolveText(meter, values);
    const measured = measureText(meter, value);
    contentWidth = measured.width;
    contentHeight = measured.height;
    text = { value, x: 0, y: 0, align: 'left', baseline: 'top' };
  } else if (meter.type === 'image') {
    const size = imageSize(meter);
    contentWidth = size?.width ?? 0;
    contentHeight = size?.height ?? 0;
  } else if (meter.type === 'bar') {
    contentWidth = DEFAULT_BAR_WIDTH;
    contentHeight = DEFAULT_BAR_HEIGHT;
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
  }

  return {
    meter,
    x,
    y,
    width: Math.ceil(width),
    height: Math.ceil(height),
    content,
    text,
  };
}
