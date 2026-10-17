export type Relative = 'absolute' | 'position' | 'edge';

export interface Dimension {
  value: number;
  relative: Relative;
}

export interface Color {
  r: number;
  g: number;
  b: number;
  a: number;
}

export interface CommonMeter {
  name: string;
  kind: string;
  x: Dimension;
  y: Dimension;
  width: number | null;
  height: number | null;
  padding: [number, number, number, number];
  hidden: boolean;
  solid_color: Color | null;
  solid_color2: Color | null;
  gradient_angle: number;
  group: string[];
  measure_names: string[];
}

export interface StringMeter extends CommonMeter {
  type: 'string';
  text: string | null;
  font_face: string;
  font_size: number;
  font_color: Color;
  font_weight: number;
  string_align: string;
  string_case: string;
  string_style: string;
  string_effect: string;
  prefix: string;
  postfix: string;
  num_of_decimals: number;
  angle: number;
}

export interface ImageMeter extends CommonMeter {
  type: 'image';
  image: string | null;
  preserve_aspect_ratio: number;
  tile: boolean;
  scale_margins: [number, number, number, number];
}

export interface BarMeter extends CommonMeter {
  type: 'bar';
  color: Color;
  orientation: string;
  flip: boolean;
  border: number;
  value: number;
}

export interface UnsupportedMeter extends CommonMeter {
  type: 'unsupported';
  kind: string;
}

export type RenderMeter =
  StringMeter | ImageMeter | BarMeter | UnsupportedMeter;

export interface Background {
  mode: string;
  image: string | null;
  color: Color;
  color2: Color | null;
  gradient_angle: number;
}

export interface RenderModel {
  name: string;
  width: number | null;
  height: number | null;
  update_ms: number;
  dynamic_window_size: boolean;
  background: Background;
  meters: RenderMeter[];
  local_fonts: string[];
  measure_values: Record<string, string>;
}

export function cssColor(color: Color): string {
  return `rgba(${color.r}, ${color.g}, ${color.b}, ${color.a / 255})`;
}
