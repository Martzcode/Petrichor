import { Color, StringMeter } from './skin-model';

export interface GradientStop {
  position: number;
  color: Color;
}

export interface GradientStyle {
  angle: number;
  stops: GradientStop[];
}

export interface CharacterSpacing {
  leading: number;
  trailing: number;
  advance: number | null;
}

export interface ShadowStyle {
  offsetX: number;
  offsetY: number;
  blur: number;
  color: Color;
}

export interface TextStyle {
  color: Color;
  face: string;
  size: number;
  weight: number;
  italic: boolean;
  underline: boolean;
  strikethrough: boolean;
  gradient: GradientStyle | null;
  shadow: ShadowStyle | null;
  spacing: CharacterSpacing | null;
}

export interface TextRun {
  text: string;
  style: TextStyle;
}

type CaseMode = 'lower' | 'upper' | 'proper' | 'sentence';

interface ParsedSetting {
  style: Partial<TextStyle>;
  caseMode: CaseMode | null;
}

export function baseStyle(meter: StringMeter): TextStyle {
  const style = meter.string_style.toUpperCase();
  return {
    color: meter.font_color,
    face: meter.font_face,
    size: meter.font_size,
    weight:
      meter.font_weight > 0
        ? meter.font_weight
        : style.includes('BOLD')
          ? 700
          : 400,
    italic: style.includes('ITALIC') || style.includes('OBLIQUE'),
    underline: false,
    strikethrough: false,
    gradient: null,
    shadow: null,
    spacing: null,
  };
}

export function buildTextRuns(meter: StringMeter, text: string): TextRun[] {
  const base = baseStyle(meter);
  const settings = meter.inline_settings ?? [];
  const patterns = meter.inline_patterns ?? [];
  const ranges: {
    start: number;
    end: number;
    style: Partial<TextStyle>;
    caseMode: CaseMode | null;
  }[] = [];

  for (
    let index = 0;
    index < Math.min(settings.length, patterns.length);
    index += 1
  ) {
    const parsed = parseSetting(settings[index]);
    if (!parsed) continue;
    const regex = compilePattern(patterns[index]);
    if (!regex) continue;
    collectRanges(text, regex, parsed, ranges);
  }

  if (!ranges.length) return text ? [{ text, style: base }] : [];

  const boundaries = new Set<number>([0, text.length]);
  for (const range of ranges) {
    boundaries.add(range.start);
    boundaries.add(range.end);
  }
  const points = [...boundaries].sort((a, b) => a - b);
  const runs: TextRun[] = [];

  for (let index = 0; index < points.length - 1; index += 1) {
    const start = points[index];
    const end = points[index + 1];
    if (start === end) continue;

    let style = base;
    let caseMode: CaseMode | null = null;
    for (const range of ranges) {
      if (range.start <= start && range.end >= end) {
        style = { ...style, ...range.style };
        if (range.caseMode) caseMode = range.caseMode;
      }
    }

    runs.push({
      text: applyCase(text.slice(start, end), caseMode),
      style,
    });
  }

  return runs;
}

function collectRanges(
  text: string,
  regex: RegExp,
  parsed: ParsedSetting,
  ranges: {
    start: number;
    end: number;
    style: Partial<TextStyle>;
    caseMode: CaseMode | null;
  }[],
): void {
  regex.lastIndex = 0;
  let match: RegExpExecArray | null;
  while ((match = regex.exec(text)) !== null) {
    let start = match.index;
    let end = match.index + match[0].length;
    const indices = match.indices;
    if (indices && indices.length > 1) {
      for (let group = 1; group < indices.length; group += 1) {
        if (indices[group]) {
          [start, end] = indices[group] as [number, number];
          break;
        }
      }
    }
    if (end > start) {
      ranges.push({
        start,
        end,
        style: parsed.style,
        caseMode: parsed.caseMode,
      });
    }
    if (match[0].length === 0) regex.lastIndex += 1;
  }
}

function compilePattern(pattern: string): RegExp | null {
  if (!pattern) return null;
  try {
    return new RegExp(pattern, 'gud');
  } catch {
    try {
      return new RegExp(pattern, 'gu');
    } catch {
      return null;
    }
  }
}

function applyCase(value: string, mode: CaseMode | null): string {
  switch (mode) {
    case 'lower':
      return value.toLowerCase();
    case 'upper':
      return value.toUpperCase();
    case 'proper':
      return value.replace(/\b\w/g, (character) => character.toUpperCase());
    case 'sentence':
      return value
        .toLowerCase()
        .replace(/(^\s*\w|[.!?]\s+\w)/g, (character) =>
          character.toUpperCase(),
        );
    default:
      return value;
  }
}

function parseSetting(value: string): ParsedSetting | null {
  const trimmed = value.trim();
  if (!trimmed) return null;
  const separator = trimmed.search(/\s/);
  const name = (
    separator < 0 ? trimmed : trimmed.slice(0, separator)
  ).toUpperCase();
  const argument = separator < 0 ? '' : trimmed.slice(separator + 1).trim();

  switch (name) {
    case 'NONE':
      return null;
    case 'CASE': {
      const mode = argument.toLowerCase();
      if (
        mode === 'lower' ||
        mode === 'upper' ||
        mode === 'proper' ||
        mode === 'sentence'
      ) {
        return { style: {}, caseMode: mode };
      }
      return null;
    }
    case 'COLOR': {
      const color = parseColor(argument);
      return color ? { style: { color }, caseMode: null } : null;
    }
    case 'FACE':
      return argument ? { style: { face: argument }, caseMode: null } : null;
    case 'SIZE': {
      const size = Number.parseFloat(argument);
      return Number.isFinite(size) ? { style: { size }, caseMode: null } : null;
    }
    case 'WEIGHT': {
      const weight = Number.parseInt(argument, 10);
      return Number.isFinite(weight)
        ? { style: { weight }, caseMode: null }
        : null;
    }
    case 'ITALIC':
    case 'OBLIQUE':
      return { style: { italic: true }, caseMode: null };
    case 'UNDERLINE':
      return { style: { underline: true }, caseMode: null };
    case 'STRIKETHROUGH':
      return { style: { strikethrough: true }, caseMode: null };
    case 'SHADOW': {
      const parts = argument.split(',').map((part) => part.trim());
      if (parts.length < 4) return null;
      const [offsetX, offsetY, blur] = parts.slice(0, 3).map(Number.parseFloat);
      const color = parseColor(parts[3]);
      if (!color || !Number.isFinite(offsetX) || !Number.isFinite(offsetY)) {
        return null;
      }
      return {
        style: { shadow: { offsetX, offsetY, blur: blur || 0, color } },
        caseMode: null,
      };
    }
    case 'CHARACTERSPACING': {
      const parts = argument.split(',').map((part) => part.trim());
      const leading =
        parts[0] && parts[0] !== '*' ? Number.parseFloat(parts[0]) : 0;
      const trailing =
        parts[1] && parts[1] !== '*' ? Number.parseFloat(parts[1]) : 0;
      const advance =
        parts[2] && parts[2] !== '*' ? Number.parseFloat(parts[2]) : null;
      if (!Number.isFinite(leading) || !Number.isFinite(trailing)) return null;
      return {
        style: {
          spacing: {
            leading,
            trailing,
            advance: Number.isFinite(advance) ? advance : null,
          },
        },
        caseMode: null,
      };
    }
    case 'GRADIENTCOLOR':
    case 'GRADIENTCOLOR1': {
      const gradient = parseGradient(argument);
      return gradient ? { style: { gradient }, caseMode: null } : null;
    }
    default:
      return null;
  }
}

function parseGradient(value: string): GradientStyle | null {
  const parts = value.split('|').map((part) => part.trim());
  if (parts.length < 2) return null;
  const angle = Number.parseFloat(parts[0]);
  if (!Number.isFinite(angle)) return null;
  const stops: GradientStop[] = [];
  for (const part of parts.slice(1)) {
    const [colorText, positionText] = part
      .split(';')
      .map((item) => item.trim());
    const color = parseColor(colorText);
    const position = Number.parseFloat(positionText ?? '0');
    if (color)
      stops.push({ color, position: Number.isFinite(position) ? position : 0 });
  }
  return stops.length ? { angle, stops } : null;
}

export function parseColor(value: string): Color | null {
  const hex = value.trim().replace(/^#/, '');
  if (!/^[0-9a-fA-F]{6}([0-9a-fA-F]{2})?$/.test(hex)) return null;
  return {
    r: Number.parseInt(hex.slice(0, 2), 16),
    g: Number.parseInt(hex.slice(2, 4), 16),
    b: Number.parseInt(hex.slice(4, 6), 16),
    a: hex.length === 8 ? Number.parseInt(hex.slice(6, 8), 16) : 255,
  };
}
