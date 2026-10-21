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

export interface MeasureValue {
  number: number;
  string: string | null;
  min: number | null;
  max: number | null;
}

export interface MeasureSnapshot {
  strings: Record<string, string>;
  values: Record<string, MeasureValue>;
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
  left_mouse_up_action: string | null;
  mouse_over_action: string | null;
  mouse_leave_action: string | null;
  container?: string | null;
  transformation_matrix?: number[] | null;
  bevel_type?: string;
  bevel_color?: Color;
  bevel_color2?: Color;
  anti_alias?: boolean;
  tooltip_text?: string | null;
  tooltip_title?: string | null;
  tooltip_icon?: string | null;
  tooltip_width?: number;
  tooltip_type?: boolean;
  tooltip_hidden?: boolean;
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
  inline_settings: string[];
  inline_patterns: string[];
}

export interface ImageMeter extends CommonMeter {
  type: 'image';
  image: string | null;
  preserve_aspect_ratio: number;
  tile: boolean;
  scale_margins: [number, number, number, number];
  crop?: [number, number, number, number] | null;
  greyscale?: boolean;
  tint?: Color | null;
  color_matrix?: number[] | null;
  exif?: boolean;
  flip?: string;
  rotate?: number;
}

export interface BarMeter extends CommonMeter {
  type: 'bar';
  color: Color;
  orientation: string;
  flip: boolean;
  border: number;
  value: number;
}

export interface HistogramMeter extends CommonMeter {
  type: 'histogram';
  primary_color: Color;
  secondary_color: Color;
  both_color: Color;
  primary_image: string | null;
  secondary_image: string | null;
  both_image: string | null;
  auto_scale: boolean;
  flip: boolean;
  graph_start: string;
  graph_orientation: string;
  primary: number;
  secondary: number;
  primary_measure: string | null;
  secondary_measure: string | null;
}

export interface LineMeter extends CommonMeter {
  type: 'line';
  line_count: number;
  colors: Color[];
  scales: number[];
  flip: boolean;
  auto_scale: boolean;
  line_width: number;
  horizontal_lines: number;
  horizontal_color: Color;
  graph_start: string;
  graph_orientation: string;
  transform_stroke: string;
  values: number[];
}

export interface RotatorMeter extends CommonMeter {
  type: 'rotator';
  image: string | null;
  offset_x: number;
  offset_y: number;
  start_angle: number;
  rotation_angle: number;
  value_remainder: number;
  value: number;
}

export interface RoundLineMeter extends CommonMeter {
  type: 'roundline';
  line_width: number;
  line_length: number;
  line_start: number;
  start_angle: number;
  rotation_angle: number;
  value_remainder: number;
  line_color: Color;
  solid: boolean;
  control_angle: boolean;
  control_start: boolean;
  control_length: boolean;
  start_shift: number;
  length_shift: number;
  value: number;
}

export interface BitmapMeter extends CommonMeter {
  type: 'bitmap';
  image: string | null;
  frames: number;
  zero_frame: boolean;
  separation: number;
  extend: boolean;
  digits: number;
  transition_frames: number;
  align: string;
  value: number;
  text: string;
}

export interface ButtonMeter extends CommonMeter {
  type: 'button';
  image: string | null;
  command: string | null;
}

export interface ShapePart {
  kind: string;
  params: number[];
  path: string | null;
  fill_color: Color | null;
  fill_color2: Color | null;
  stroke_color: Color | null;
  stroke_color2: Color | null;
  stroke_width: number | null;
  stroke_dash: number[] | null;
  offset?: [number, number] | null;
  rotate?: number | null;
  scale?: [number, number] | null;
  skew?: [number, number] | null;
  anchor?: [number, number] | null;
  combine?: ShapeCombine | null;
}

export interface ShapeCombine {
  parent: number;
  operations: ShapeCombineOp[];
}

export interface ShapeCombineOp {
  mode: string;
  shape: number;
}

export interface ShapeMeter extends CommonMeter {
  type: 'shape';
  shapes: ShapePart[];
  fill_color: Color;
  fill_color2: Color | null;
  stroke_color: Color;
  stroke_color2: Color | null;
  stroke_width: number;
  stroke_dash: number[];
}

export interface UnsupportedMeter extends CommonMeter {
  type: 'unsupported';
  kind: string;
}

export type RenderMeter =
  | StringMeter
  | ImageMeter
  | BarMeter
  | HistogramMeter
  | LineMeter
  | RotatorMeter
  | RoundLineMeter
  | BitmapMeter
  | ButtonMeter
  | ShapeMeter
  | UnsupportedMeter;

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
  tooltip_hidden?: boolean;
  on_refresh_action?: string | null;
  background: Background;
  meters: RenderMeter[];
  local_fonts: string[];
  measure_values: Record<string, string>;
}

export function cssColor(color: Color): string {
  return `rgba(${color.r}, ${color.g}, ${color.b}, ${color.a / 255})`;
}
