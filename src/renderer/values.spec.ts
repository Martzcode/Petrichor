import {
  BarMeter,
  HistogramMeter,
  LineMeter,
  MeasureValue,
  RenderModel,
} from './skin-model';
import {
  appendHistories,
  measureRatio,
  MeterHistories,
  resetHistories,
  updateMeterValues,
} from './values';

function common(name: string, kind: string) {
  return {
    name,
    kind,
    x: { value: 0, relative: 'absolute' as const },
    y: { value: 0, relative: 'absolute' as const },
    width: 100,
    height: 40,
    padding: [0, 0, 0, 0] as [number, number, number, number],
    hidden: false,
    solid_color: null,
    solid_color2: null,
    gradient_angle: 0,
    group: [],
    left_mouse_up_action: null,
    mouse_over_action: null,
    mouse_leave_action: null,
  };
}

function bar(): BarMeter {
  return {
    ...common('Bar', 'Bar'),
    type: 'bar',
    measure_names: ['CPU'],
    color: { r: 0, g: 200, b: 100, a: 255 },
    orientation: 'HORIZONTAL',
    flip: false,
    border: 0,
    value: 0,
  };
}

function histogram(): HistogramMeter {
  return {
    ...common('Histogram', 'Histogram'),
    type: 'histogram',
    measure_names: ['CPU', 'RAM'],
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
    primary: 0,
    secondary: 0,
    primary_measure: 'CPU',
    secondary_measure: 'RAM',
  };
}

function line(): LineMeter {
  return {
    ...common('Line', 'Line'),
    type: 'line',
    measure_names: ['CPU', 'RAM'],
    line_count: 2,
    colors: [
      { r: 255, g: 255, b: 255, a: 255 },
      { r: 0, g: 255, b: 0, a: 255 },
    ],
    scales: [1, 1],
    flip: false,
    auto_scale: false,
    line_width: 1,
    horizontal_lines: 0,
    horizontal_color: { r: 128, g: 128, b: 128, a: 128 },
    graph_start: 'RIGHT',
    graph_orientation: 'HORIZONTAL',
    transform_stroke: 'NORMAL',
    values: [0, 0],
  };
}

function model(): RenderModel {
  return {
    name: 'Demo',
    width: null,
    height: null,
    update_ms: 1000,
    dynamic_window_size: true,
    background: {
      mode: 'SOLID',
      image: null,
      color: { r: 0, g: 0, b: 0, a: 255 },
      color2: null,
      gradient_angle: 0,
    },
    meters: [bar(), histogram(), line()],
    local_fonts: [],
    measure_values: {},
  };
}

function measure(number: number, min = 0, max = 100): MeasureValue {
  return { number, string: null, min, max };
}

describe('measureRatio', () => {
  it('normalizes and clamps within the measure range', () => {
    expect(measureRatio(measure(50))).toBe(0.5);
    expect(measureRatio(measure(150))).toBe(1);
    expect(measureRatio(measure(-10))).toBe(0);
    expect(measureRatio(measure(15, 10, 20))).toBe(0.5);
  });

  it('returns zero without a measure or range', () => {
    expect(measureRatio(null)).toBe(0);
    expect(measureRatio(measure(5, 4, 4))).toBe(0);
  });
});

describe('updateMeterValues', () => {
  it('updates bars, histograms and lines from raw measures', () => {
    const skin = model();
    updateMeterValues(skin, {
      cpu: measure(50),
      ram: measure(25),
    });

    expect((skin.meters[0] as BarMeter).value).toBe(0.5);
    expect((skin.meters[1] as HistogramMeter).primary).toBe(0.5);
    expect((skin.meters[1] as HistogramMeter).secondary).toBe(0.25);
    expect((skin.meters[2] as LineMeter).values).toEqual([0.5, 0.25]);
  });
});

describe('histories', () => {
  it('appends one sample per channel and resets', () => {
    const histories: MeterHistories = new Map();
    const skin = model();
    updateMeterValues(skin, { cpu: measure(50), ram: measure(25) });
    appendHistories(histories, skin);
    updateMeterValues(skin, { cpu: measure(80), ram: measure(40) });
    appendHistories(histories, skin);

    expect(histories.get('Histogram')).toEqual([
      [0.5, 0.8],
      [0.25, 0.4],
    ]);
    expect(histories.get('Line')).toEqual([
      [0.5, 0.8],
      [0.25, 0.4],
    ]);

    resetHistories(histories);
    expect(histories.size).toBe(0);
  });
});
