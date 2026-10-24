import { layoutSkin } from './layout';
import { MeterTransition, renderSkin } from './renderer';
import {
  BarMeter,
  BitmapMeter,
  ButtonMeter,
  HistogramMeter,
  ImageMeter,
  LineMeter,
  RenderModel,
  RotatorMeter,
  RoundLineMeter,
  ShapeMeter,
  StringMeter,
} from './skin-model';
import { MeterHistories } from './values';

interface Call {
  method: string;
  args: unknown[];
}

class FakeContext {
  calls: Call[] = [];
  font = '';
  fillStyle: string | CanvasGradient = '';
  strokeStyle: string | CanvasGradient = '';
  textAlign = 'left';
  textBaseline = 'alphabetic';
  lineWidth = 1;
  filter = 'none';
  globalCompositeOperation: GlobalCompositeOperation = 'source-over';
  canvas = { width: 0, height: 0 } as HTMLCanvasElement;

  clearRect(...args: unknown[]): void {
    this.record('clearRect', args);
  }
  fillRect(...args: unknown[]): void {
    this.record('fillRect', args);
  }
  strokeRect(...args: unknown[]): void {
    this.record('strokeRect', args);
  }
  beginPath(): void {
    this.record('beginPath', []);
  }
  rect(...args: unknown[]): void {
    this.record('rect', args);
  }
  clip(): void {
    this.record('clip', []);
  }
  moveTo(...args: unknown[]): void {
    this.record('moveTo', args);
  }
  lineTo(...args: unknown[]): void {
    this.record('lineTo', args);
  }
  stroke(): void {
    this.record('stroke', []);
  }
  arc(...args: unknown[]): void {
    this.record('arc', args);
  }
  ellipse(...args: unknown[]): void {
    this.record('ellipse', args);
  }
  quadraticCurveTo(...args: unknown[]): void {
    this.record('quadraticCurveTo', args);
  }
  bezierCurveTo(...args: unknown[]): void {
    this.record('bezierCurveTo', args);
  }
  closePath(): void {
    this.record('closePath', []);
  }
  fill(): void {
    this.record('fill', []);
  }
  getImageData(...args: unknown[]): ImageData {
    this.record('getImageData', args);
    return {
      data: new Uint8ClampedArray([255, 0, 0, 255]),
      width: 1,
      height: 1,
      colorSpace: 'srgb',
    } as ImageData;
  }
  putImageData(...args: unknown[]): void {
    this.record('putImageData', args);
  }
  setLineDash(...args: unknown[]): void {
    this.record('setLineDash', args);
  }
  transform(...args: unknown[]): void {
    this.record('transform', args);
  }
  scale(...args: unknown[]): void {
    this.record('scale', args);
  }
  drawImage(...args: unknown[]): void {
    this.record('drawImage', args);
  }
  fillText(...args: unknown[]): void {
    this.record('fillText', args);
  }
  save(): void {
    this.record('save', []);
  }
  restore(): void {
    this.record('restore', []);
  }
  translate(...args: unknown[]): void {
    this.record('translate', args);
  }
  rotate(...args: unknown[]): void {
    this.record('rotate', args);
  }
  createLinearGradient(...args: unknown[]): CanvasGradient {
    this.record('createLinearGradient', args);
    return { addColorStop: () => undefined } as unknown as CanvasGradient;
  }
  measureText(text: string): TextMetrics {
    return {
      width: text.length * 6,
      actualBoundingBoxAscent: 8,
      actualBoundingBoxDescent: 2,
    } as TextMetrics;
  }

  private record(method: string, args: unknown[]): void {
    this.calls.push({ method, args });
  }

  of(method: string): Call[] {
    return this.calls.filter((call) => call.method === method);
  }
}

function stringMeter(overrides: Partial<StringMeter> = {}): StringMeter {
  return {
    type: 'string',
    name: 'Label',
    kind: 'String',
    x: { value: 0, relative: 'absolute' },
    y: { value: 0, relative: 'absolute' },
    width: null,
    height: null,
    padding: [0, 0, 0, 0],
    hidden: false,
    solid_color: null,
    solid_color2: null,
    gradient_angle: 0,
    group: [],
    measure_names: [],
    left_mouse_up_action: null,
    mouse_over_action: null,
    mouse_leave_action: null,
    text: 'hello',
    font_face: 'sans-serif',
    font_size: 12,
    font_color: { r: 255, g: 255, b: 255, a: 255 },
    font_weight: -1,
    string_align: 'LEFT',
    string_case: 'NONE',
    string_style: 'NORMAL',
    string_effect: 'NONE',
    prefix: '',
    postfix: '',
    num_of_decimals: -1,
    angle: 0,
    inline_settings: [],
    inline_patterns: [],
    ...overrides,
  };
}

function imageMeter(overrides: Partial<ImageMeter> = {}): ImageMeter {
  return {
    ...commonMeter('Picture', 'Image'),
    type: 'image',
    image: '/tmp/picture.png',
    preserve_aspect_ratio: 0,
    tile: false,
    scale_margins: [0, 0, 0, 0],
    ...overrides,
  };
}

function barMeter(overrides: Partial<BarMeter> = {}): BarMeter {
  return {
    type: 'bar',
    name: 'Bar',
    kind: 'Bar',
    x: { value: 0, relative: 'absolute' },
    y: { value: 0, relative: 'absolute' },
    width: 100,
    height: 10,
    padding: [0, 0, 0, 0],
    hidden: false,
    solid_color: null,
    solid_color2: null,
    gradient_angle: 0,
    group: [],
    measure_names: [],
    left_mouse_up_action: null,
    mouse_over_action: null,
    mouse_leave_action: null,
    color: { r: 0, g: 200, b: 100, a: 255 },
    orientation: 'HORIZONTAL',
    flip: false,
    border: 0,
    value: 0.25,
    ...overrides,
  };
}

function histogramMeter(
  overrides: Partial<HistogramMeter> = {},
): HistogramMeter {
  return {
    type: 'histogram',
    name: 'Histogram',
    kind: 'Histogram',
    x: { value: 0, relative: 'absolute' },
    y: { value: 0, relative: 'absolute' },
    width: 100,
    height: 40,
    padding: [0, 0, 0, 0],
    hidden: false,
    solid_color: null,
    solid_color2: null,
    gradient_angle: 0,
    group: [],
    measure_names: ['Primary'],
    left_mouse_up_action: null,
    mouse_over_action: null,
    mouse_leave_action: null,
    primary_color: { r: 0, g: 255, b: 0, a: 255 },
    secondary_color: { r: 255, g: 0, b: 0, a: 255 },
    both_color: { r: 255, g: 255, b: 0, a: 255 },
    primary_image: null,
    secondary_image: null,
    both_image: null,
    auto_scale: false,
    flip: false,
    graph_start: 'RIGHT',
    graph_orientation: 'VERTICAL',
    primary: 0.5,
    secondary: 0.25,
    primary_measure: 'Primary',
    secondary_measure: 'Secondary',
    ...overrides,
  };
}

function lineMeter(overrides: Partial<LineMeter> = {}): LineMeter {
  return {
    type: 'line',
    name: 'Line',
    kind: 'Line',
    x: { value: 0, relative: 'absolute' },
    y: { value: 0, relative: 'absolute' },
    width: 100,
    height: 40,
    padding: [0, 0, 0, 0],
    hidden: false,
    solid_color: null,
    solid_color2: null,
    gradient_angle: 0,
    group: [],
    measure_names: ['Primary'],
    left_mouse_up_action: null,
    mouse_over_action: null,
    mouse_leave_action: null,
    line_count: 1,
    colors: [{ r: 255, g: 255, b: 255, a: 255 }],
    scales: [1],
    flip: false,
    auto_scale: false,
    line_width: 1,
    horizontal_lines: 0,
    horizontal_color: { r: 128, g: 128, b: 128, a: 128 },
    graph_start: 'RIGHT',
    graph_orientation: 'HORIZONTAL',
    transform_stroke: 'NORMAL',
    values: [0.5],
    ...overrides,
  };
}

function rotatorMeter(overrides: Partial<RotatorMeter> = {}): RotatorMeter {
  return {
    ...commonMeter('Rotator', 'Rotator'),
    type: 'rotator',
    image: '/tmp/dial.png',
    offset_x: 0,
    offset_y: 0,
    start_angle: 0,
    rotation_angle: Math.PI * 2,
    value_remainder: 0,
    value: 0.25,
    ...overrides,
  };
}

function roundLineMeter(
  overrides: Partial<RoundLineMeter> = {},
): RoundLineMeter {
  return {
    ...commonMeter('RoundLine', 'RoundLine'),
    type: 'roundline',
    line_width: 4,
    line_length: 20,
    line_start: -1,
    start_angle: 0,
    rotation_angle: Math.PI * 2,
    value_remainder: 0,
    line_color: { r: 255, g: 255, b: 255, a: 255 },
    solid: false,
    control_angle: true,
    control_start: false,
    control_length: false,
    start_shift: 0,
    length_shift: 0,
    value: 0.5,
    ...overrides,
  };
}

function bitmapMeter(overrides: Partial<BitmapMeter> = {}): BitmapMeter {
  return {
    ...commonMeter('Bitmap', 'Bitmap'),
    type: 'bitmap',
    image: '/tmp/frames.png',
    frames: 10,
    zero_frame: true,
    separation: 0,
    extend: false,
    digits: 0,
    transition_frames: 0,
    align: 'LEFT',
    value: 0.5,
    text: '5',
    ...overrides,
  };
}

function buttonMeter(overrides: Partial<ButtonMeter> = {}): ButtonMeter {
  return {
    ...commonMeter('Button', 'Button'),
    type: 'button',
    image: '/tmp/button.png',
    command: '[!Log "ok"]',
    width: 20,
    height: 10,
    ...overrides,
  };
}

function commonMeter(name: string, kind: string) {
  return {
    name,
    kind,
    x: { value: 0, relative: 'absolute' as const },
    y: { value: 0, relative: 'absolute' as const },
    width: 40,
    height: 40,
    padding: [0, 0, 0, 0] as [number, number, number, number],
    hidden: false,
    solid_color: null,
    solid_color2: null,
    gradient_angle: 0,
    group: [],
    measure_names: [],
    left_mouse_up_action: null,
    mouse_over_action: null,
    mouse_leave_action: null,
  };
}

function model(meters: RenderModel['meters']): RenderModel {
  return {
    name: 'Demo',
    width: null,
    height: null,
    update_ms: 1000,
    dynamic_window_size: true,
    background: {
      mode: 'SOLID',
      image: null,
      color: { r: 16, g: 20, b: 24, a: 255 },
      color2: null,
      gradient_angle: 0,
    },
    meters,
    local_fonts: [],
    blur: false,
    measure_values: {},
  };
}

function render(
  skin: RenderModel,
  histories: MeterHistories = new Map(),
  getImage: (path: string) => CanvasImageSource | null = () => null,
  hovered: string | null = null,
  pressed: string | null = null,
  values: Record<string, string> = {},
): FakeContext {
  const context = new FakeContext();
  const fake = context as unknown as CanvasRenderingContext2D;
  const layout = layoutSkin(skin, (meter, text) => ({
    width: text.length * 6,
    height: meter.font_size,
  }));
  renderSkin(fake, skin, layout, getImage, histories, {
    hovered,
    pressed,
    values,
  });
  return context;
}

const IMAGE = { width: 40, height: 20 } as CanvasImageSource;

describe('renderSkin', () => {
  it('clears and paints the background', () => {
    const context = render(model([stringMeter()]));
    expect(context.of('clearRect').length).toBe(1);
    expect(context.of('fillRect')[0].args).toEqual([0, 0, 30, 12]);
  });

  it('draws string text with the resolved value', () => {
    const context = render(model([stringMeter({ text: 'hi' })]));
    const text = context.of('fillText');
    expect(text.length).toBe(1);
    expect(text[0].args[0]).toBe('hi');
  });

  it('fills the bar proportionally', () => {
    const context = render(model([barMeter({ value: 0.25 })]));
    const fills = context.of('fillRect');
    const bar = fills[fills.length - 1].args as number[];
    expect(bar[2]).toBe(25);
    expect(bar[3]).toBe(10);
  });

  it('flips the bar horizontally', () => {
    const context = render(model([barMeter({ value: 0.5, flip: true })]));
    const fills = context.of('fillRect');
    const bar = fills[fills.length - 1].args as number[];
    expect(bar[0]).toBe(50);
  });

  it('uses a gradient when two colors are provided', () => {
    const skin = model([]);
    skin.background.color2 = { r: 32, g: 40, b: 48, a: 255 };
    const context = render(skin);
    expect(context.of('createLinearGradient').length).toBe(1);
  });

  it('draws histogram bars from primary and secondary values', () => {
    const context = render(
      model([histogramMeter({ primary: 1, secondary: 1 })]),
    );
    const fills = context.of('fillRect');
    expect(fills.length).toBe(4);
    expect(fills[1].args).toEqual([99, 0, 1, 40]);
    expect(fills[2].args).toEqual([99, 0, 1, 40]);
    expect(fills[3].args).toEqual([99, 0, 1, 40]);
  });

  it('draws line history as a polyline', () => {
    const histories: MeterHistories = new Map([['Line', [[0, 0.5, 1]]]]);
    const context = render(
      model([lineMeter({ width: 100, height: 40 })]),
      histories,
    );
    expect(context.of('moveTo').length).toBe(1);
    expect(context.of('lineTo').length).toBe(2);
    expect(context.of('stroke').length).toBe(1);
  });

  it('draws horizontal reference lines', () => {
    const histories: MeterHistories = new Map([['Line', [[0, 0.5]]]]);
    const context = render(
      model([lineMeter({ horizontal_lines: 2 })]),
      histories,
    );
    expect(context.of('moveTo').length).toBe(3);
    expect(context.of('stroke').length).toBe(3);
  });

  it('rotates and draws a rotator image', () => {
    const context = render(model([rotatorMeter()]), new Map(), () => IMAGE);
    expect(context.of('translate').length).toBe(1);
    expect(context.of('rotate').length).toBe(1);
    expect(context.of('drawImage').length).toBe(1);
  });

  it('selects a dynamic image variant from measure values', () => {
    const paths: string[] = [];
    const context = render(
      model([
        imageMeter({
          image: null,
          image_measure: 'WeatherIcon',
          image_variants: ['/tmp/0.png', '/tmp/1.png'],
        }),
      ]),
      new Map(),
      (path) => {
        paths.push(path);
        return IMAGE;
      },
      null,
      null,
      { WeatherIcon: '1' },
    );
    expect(paths).toEqual(['/tmp/1.png']);
    expect(context.of('drawImage').length).toBe(1);
  });

  it('crops and flips images', () => {
    const context = render(
      model([
        imageMeter({
          crop: [2, 3, 10, 10],
          flip: 'HORIZONTAL',
        }),
      ]),
      new Map(),
      () => IMAGE,
    );
    const draw = context.of('drawImage')[0];
    expect(draw.args.slice(1, 5)).toEqual([2, 3, 10, 10]);
    expect(context.of('scale')[0].args).toEqual([-1, 1]);
  });

  it('draws nine-slice images', () => {
    const context = render(
      model([imageMeter({ scale_margins: [5, 5, 5, 5] })]),
      new Map(),
      () => IMAGE,
    );
    expect(context.of('drawImage').length).toBe(9);
  });

  it('draws open arcs and curves', () => {
    const base = stringMeter();
    const shape: ShapeMeter = {
      ...base,
      type: 'shape',
      shapes: [
        {
          kind: 'arc',
          params: [0, 0, 20, 0, 10, 10, 0, 1, 0, 0],
          path: null,
          fill_color: null,
          fill_color2: null,
          stroke_color: { r: 255, g: 255, b: 255, a: 255 },
          stroke_color2: null,
          stroke_width: 1,
          stroke_dash: null,
        },
        {
          kind: 'curve',
          params: [0, 0, 20, 20, 10, 0, 10, 20, 0],
          path: null,
          fill_color: null,
          fill_color2: null,
          stroke_color: { r: 255, g: 255, b: 255, a: 255 },
          stroke_color2: null,
          stroke_width: 1,
          stroke_dash: null,
        },
      ],
      fill_color: { r: 0, g: 0, b: 0, a: 255 },
      fill_color2: null,
      stroke_color: { r: 255, g: 255, b: 255, a: 255 },
      stroke_color2: null,
      stroke_width: 1,
      stroke_dash: [],
    };
    const context = render(model([shape]));
    expect(context.of('ellipse').length).toBe(1);
    expect(context.of('bezierCurveTo')[0].args).toEqual([
      10, 0, 10, 20, 20, 20,
    ]);
    expect(context.of('fill').length).toBe(0);
    expect(context.of('stroke').length).toBe(2);
  });

  it('crops aspect-filled images', () => {
    const context = render(
      model([imageMeter({ preserve_aspect_ratio: 2 })]),
      new Map(),
      () => IMAGE,
    );
    expect(context.of('clip').length).toBe(1);
    expect(context.of('restore').length).toBeGreaterThanOrEqual(1);
  });

  it('applies a color matrix to images', () => {
    const offscreen = new FakeContext();
    offscreen.canvas = { width: 40, height: 20 } as HTMLCanvasElement;
    const spy = vi
      .spyOn(document, 'createElement')
      .mockReturnValue(offscreen.canvas as HTMLCanvasElement);
    (
      offscreen.canvas as unknown as { getContext: () => FakeContext }
    ).getContext = () => offscreen;
    try {
      render(
        model([
          imageMeter({
            color_matrix: [
              1, 0, 0, 0, 0, 1, 0, 0, 0, 0, 1, 0, 0, 0, 0, 1, 0.1, 0.2, 0.3, 0,
            ],
          }),
        ]),
        new Map(),
        () => IMAGE,
      );
      expect(offscreen.of('getImageData').length).toBe(1);
      expect(offscreen.of('putImageData').length).toBe(1);
    } finally {
      spy.mockRestore();
    }
  });

  it('draws a round line arc', () => {
    const context = render(model([roundLineMeter()]));
    const arc = context.of('arc');
    expect(arc.length).toBe(1);
    expect(arc[0].args[4]).toBeCloseTo(Math.PI);
    expect(context.of('stroke').length).toBe(1);
  });

  it('draws a bitmap frame', () => {
    const context = render(
      model([bitmapMeter({ value: 1 })]),
      new Map(),
      () => IMAGE,
    );
    const calls = context.of('drawImage');
    expect(calls.length).toBe(1);
    expect(calls[0].args[1]).toBe(36);
  });

  it('draws bitmap digits', () => {
    const context = render(
      model([bitmapMeter({ digits: 1, text: '12' })]),
      new Map(),
      () => IMAGE,
    );
    const calls = context.of('drawImage');
    expect(calls.length).toBe(2);
    expect(calls[0].args[1]).toBe(4);
    expect(calls[1].args[1]).toBe(8);
  });

  it('blends bitmap transition frames', () => {
    const transitions = new Map<string, MeterTransition>();
    const drawAt = (value: number, now: number): FakeContext => {
      const skin = model([
        bitmapMeter({ value, frames: 10, transition_frames: 2 }),
      ]);
      const context = new FakeContext();
      const layout = layoutSkin(skin, (meter, text) => ({
        width: text.length * 6,
        height: meter.font_size,
      }));
      renderSkin(
        context as unknown as CanvasRenderingContext2D,
        skin,
        layout,
        () => IMAGE,
        new Map(),
        { hovered: null, pressed: null, transitions, now },
      );
      return context;
    };

    expect(drawAt(0, 0).of('drawImage')[0].args[1]).toBe(0);
    expect(drawAt(1, 80).of('drawImage')[0].args[1]).toBe(0);
    expect(drawAt(1, 160).of('drawImage')[0].args[1]).toBe(16);
    expect(drawAt(1, 400).of('drawImage')[0].args[1]).toBe(24);
  });

  it('selects button frames for hover and press', () => {
    const normal = render(
      model([buttonMeter()]),
      new Map(),
      () => ({ width: 30, height: 10 }) as CanvasImageSource,
    );
    expect(normal.of('drawImage')[0].args[1]).toBe(0);

    const hovered = render(
      model([buttonMeter()]),
      new Map(),
      () => ({ width: 30, height: 10 }) as CanvasImageSource,
      'Button',
    );
    expect(hovered.of('drawImage')[0].args[1]).toBe(20);

    const pressed = render(
      model([buttonMeter()]),
      new Map(),
      () => ({ width: 30, height: 10 }) as CanvasImageSource,
      'Button',
      'Button',
    );
    expect(pressed.of('drawImage')[0].args[1]).toBe(10);
  });

  it('draws shape meters', () => {
    const base = stringMeter();
    const shape: ShapeMeter = {
      ...base,
      type: 'shape',
      shapes: [
        {
          kind: 'rectangle',
          params: [0, 0, 20, 10, 3],
          path: null,
          fill_color: { r: 10, g: 20, b: 30, a: 255 },
          fill_color2: null,
          stroke_color: { r: 40, g: 50, b: 60, a: 255 },
          stroke_color2: null,
          stroke_width: 2,
          stroke_dash: null,
        },
      ],
      fill_color: { r: 10, g: 20, b: 30, a: 255 },
      fill_color2: null,
      stroke_color: { r: 40, g: 50, b: 60, a: 255 },
      stroke_color2: null,
      stroke_width: 2,
      stroke_dash: [],
    };
    const context = render(model([shape]));
    expect(context.of('quadraticCurveTo').length).toBe(4);
    expect(context.of('fill').length).toBe(1);
    expect(context.of('stroke').length).toBe(1);
  });

  it('combines shapes through an offscreen canvas', () => {
    const base = stringMeter();
    const fill = { r: 10, g: 20, b: 30, a: 255 };
    const part = (overrides: Partial<ShapeMeter['shapes'][number]>) => ({
      kind: 'rectangle',
      params: [0, 0, 10, 10],
      path: null,
      fill_color: fill,
      fill_color2: null,
      stroke_color: fill,
      stroke_color2: null,
      stroke_width: 1,
      stroke_dash: null,
      ...overrides,
    });
    const shape: ShapeMeter = {
      ...base,
      type: 'shape',
      shapes: [
        part({}),
        part({ kind: 'ellipse', params: [5, 5, 5, 5] }),
        part({
          kind: 'combine',
          combine: {
            parent: 0,
            operations: [{ mode: 'EXCLUDE', shape: 1 }],
          },
        }),
      ],
      fill_color: fill,
      fill_color2: null,
      stroke_color: fill,
      stroke_color2: null,
      stroke_width: 1,
      stroke_dash: [],
    };
    const offscreen = new FakeContext();
    const spy = vi
      .spyOn(document, 'createElement')
      .mockReturnValue(offscreen.canvas as HTMLCanvasElement);
    (
      offscreen.canvas as unknown as { getContext: () => FakeContext }
    ).getContext = () => offscreen;
    try {
      const context = render(model([shape]));
      expect(context.of('drawImage').length).toBe(1);
      expect(offscreen.of('fill').length).toBeGreaterThan(1);
    } finally {
      spy.mockRestore();
    }
  });

  it('applies shape transforms', () => {
    const base = stringMeter();
    const shape: ShapeMeter = {
      ...base,
      type: 'shape',
      shapes: [
        {
          kind: 'rectangle',
          params: [0, 0, 20, 10],
          path: null,
          fill_color: { r: 10, g: 20, b: 30, a: 255 },
          fill_color2: null,
          stroke_color: null,
          stroke_color2: null,
          stroke_width: 0,
          stroke_dash: null,
          offset: [5, 6],
          rotate: 90,
          scale: [2, 2],
          skew: [10, 10],
          anchor: [10, 5],
        },
      ],
      fill_color: { r: 10, g: 20, b: 30, a: 255 },
      fill_color2: null,
      stroke_color: { r: 40, g: 50, b: 60, a: 255 },
      stroke_color2: null,
      stroke_width: 0,
      stroke_dash: [],
    };
    const context = render(model([shape]));
    expect(context.of('translate').length).toBe(4);
    expect(context.of('scale')[0].args).toEqual([2, 2]);
    expect(context.of('rotate').length).toBe(1);
    expect(context.of('transform').length).toBe(1);
  });

  it('applies a transformation matrix', () => {
    const context = render(
      model([
        stringMeter({
          text: 'hi',
          transformation_matrix: [1, 0, 0, 1, 10, 20],
        }),
      ]),
    );
    expect(context.of('transform')[0].args).toEqual([1, 0, 0, 1, 10, 20]);
  });

  it('draws a bevel when requested', () => {
    const context = render(
      model([
        stringMeter({
          solid_color: { r: 20, g: 30, b: 40, a: 255 },
          bevel_type: 'UP',
        }),
      ]),
    );
    expect(context.of('stroke').length).toBe(2);
  });

  describe('meter snapshots', () => {
    const drawList = (
      skin: RenderModel,
      getImage: (path: string) => CanvasImageSource | null = () => IMAGE,
      hovered: string | null = null,
    ) => render(skin, new Map(), getImage, hovered).calls;

    it('matches the string meter snapshot', () => {
      expect(
        drawList(model([stringMeter({ text: '42', width: 30, height: 12 })])),
      ).toMatchSnapshot();
    });

    it('matches the image meter snapshot', () => {
      expect(drawList(model([imageMeter()]))).toMatchSnapshot();
    });

    it('matches the bar meter snapshot', () => {
      expect(drawList(model([barMeter()]))).toMatchSnapshot();
    });

    it('matches the histogram meter snapshot', () => {
      const histories = new Map([['Histogram', [[0.2, 0.4, 0.8]]]]);
      expect(
        render(model([histogramMeter()]), histories).calls,
      ).toMatchSnapshot();
    });

    it('matches the line meter snapshot', () => {
      const histories = new Map([['Line', [[0.2, 0.4, 0.8]]]]);
      expect(render(model([lineMeter()]), histories).calls).toMatchSnapshot();
    });

    it('matches the rotator meter snapshot', () => {
      expect(drawList(model([rotatorMeter()]))).toMatchSnapshot();
    });

    it('matches the round line meter snapshot', () => {
      expect(drawList(model([roundLineMeter()]))).toMatchSnapshot();
    });

    it('matches the bitmap meter snapshot', () => {
      expect(drawList(model([bitmapMeter({ value: 0.5 })]))).toMatchSnapshot();
    });

    it('matches the shape meter snapshot', () => {
      const fill = { r: 10, g: 20, b: 30, a: 255 };
      const base = stringMeter();
      const shape: ShapeMeter = {
        ...base,
        type: 'shape',
        shapes: [
          {
            kind: 'rectangle',
            params: [0, 0, 20, 10, 3],
            path: null,
            fill_color: fill,
            fill_color2: null,
            stroke_color: { r: 40, g: 50, b: 60, a: 255 },
            stroke_color2: null,
            stroke_width: 1,
            stroke_dash: null,
          },
          {
            kind: 'arc',
            params: [20, 0, 40, 20, 10, 10, 0, 1, 0, 0],
            path: null,
            fill_color: null,
            fill_color2: null,
            stroke_color: { r: 40, g: 50, b: 60, a: 255 },
            stroke_color2: null,
            stroke_width: 1,
            stroke_dash: null,
          },
        ],
        fill_color: fill,
        fill_color2: null,
        stroke_color: { r: 40, g: 50, b: 60, a: 255 },
        stroke_color2: null,
        stroke_width: 1,
        stroke_dash: [],
      };
      expect(drawList(model([shape]))).toMatchSnapshot();
    });

    it('matches the button meter snapshot', () => {
      expect(
        drawList(
          model([buttonMeter()]),
          () => ({ width: 30, height: 10 }) as CanvasImageSource,
          'Button',
        ),
      ).toMatchSnapshot();
    });
  });

  it('matches the reference draw list', () => {
    const skin = model([
      stringMeter({
        text: 'CPU',
        width: 60,
        height: 20,
        padding: [2, 2, 2, 2],
      }),
      barMeter({ y: { value: 4, relative: 'edge' }, value: 0.75 }),
    ]);
    const context = render(skin);
    expect(
      context.calls.map((call) => ({ method: call.method, args: call.args })),
    ).toMatchSnapshot();
  });
});
