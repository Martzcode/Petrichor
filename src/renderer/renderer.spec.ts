import { layoutSkin } from './layout';
import { renderSkin } from './renderer';
import { RenderModel, StringMeter, BarMeter } from './skin-model';

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

  clearRect(...args: unknown[]): void {
    this.record('clearRect', args);
  }
  fillRect(...args: unknown[]): void {
    this.record('fillRect', args);
  }
  strokeRect(...args: unknown[]): void {
    this.record('strokeRect', args);
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
    color: { r: 0, g: 200, b: 100, a: 255 },
    orientation: 'HORIZONTAL',
    flip: false,
    border: 0,
    value: 0.25,
    ...overrides,
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
    measure_values: {},
  };
}

function render(skin: RenderModel): FakeContext {
  const context = new FakeContext();
  const fake = context as unknown as CanvasRenderingContext2D;
  const layout = layoutSkin(skin, (meter, text) => ({
    width: text.length * 6,
    height: meter.font_size,
  }));
  renderSkin(fake, skin, layout, () => null);
  return context;
}

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
