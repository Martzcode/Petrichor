import { hitTest, layoutSkin, resolveText } from './layout';
import {
  BarMeter,
  CommonMeter,
  ImageMeter,
  RenderModel,
  StringMeter,
} from './skin-model';

function common(overrides: Partial<CommonMeter> = {}): CommonMeter {
  return {
    name: 'Meter',
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
    ...overrides,
  };
}

function stringMeter(overrides: Partial<StringMeter> = {}): StringMeter {
  return {
    type: 'string',
    text: null,
    font_face: 'sans-serif',
    font_size: 10,
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
    ...common({ kind: 'String' }),
    ...overrides,
  };
}

function barMeter(overrides: Partial<BarMeter> = {}): BarMeter {
  return {
    type: 'bar',
    color: { r: 0, g: 200, b: 100, a: 255 },
    orientation: 'HORIZONTAL',
    flip: false,
    border: 0,
    value: 0.5,
    ...common({ kind: 'Bar' }),
    ...overrides,
  };
}

function imageMeter(overrides: Partial<ImageMeter> = {}): ImageMeter {
  return {
    type: 'image',
    image: '/tmp/logo.png',
    preserve_aspect_ratio: 0,
    tile: false,
    scale_margins: [0, 0, 0, 0],
    ...common({ kind: 'Image' }),
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
      color: { r: 0, g: 0, b: 0, a: 0 },
      color2: null,
      gradient_angle: 0,
    },
    meters,
    local_fonts: [],
    blur: false,
    measure_values: { cpu: '42' },
  };
}

const measure = (width: number, height: number) => () => ({ width, height });

describe('layoutSkin', () => {
  it('sizes the skin from the meters', () => {
    const layout = layoutSkin(
      model([stringMeter({ text: 'hello' })]),
      measure(40, 12),
    );
    expect(layout.width).toBe(40);
    expect(layout.height).toBe(12);
    expect(layout.boxes[0].width).toBe(40);
  });

  it('resolves absolute and relative positions', () => {
    const layout = layoutSkin(
      model([
        stringMeter({ x: { value: 10, relative: 'absolute' } }),
        stringMeter({
          x: { value: 5, relative: 'position' },
          y: { value: 5, relative: 'edge' },
        }),
        stringMeter({ x: { value: 2, relative: 'edge' } }),
      ]),
      measure(20, 10),
    );
    expect(layout.boxes[0].x).toBe(10);
    expect(layout.boxes[1].x).toBe(15);
    expect(layout.boxes[1].y).toBe(15);
    expect(layout.boxes[2].x).toBe(37);
  });

  it('centers strings on their X anchor', () => {
    const layout = layoutSkin(
      model([
        stringMeter({
          text: 'hello',
          x: { value: 200, relative: 'absolute' },
          string_align: 'CENTER',
        }),
      ]),
      measure(40, 12),
    );
    const box = layout.boxes[0];
    expect(box.x).toBe(180);
    expect(box.text?.x).toBe(200);
    expect(box.text?.runs[0].x).toBe(180);
  });

  it('right-aligns strings on their X anchor', () => {
    const layout = layoutSkin(
      model([
        stringMeter({
          text: 'hello',
          x: { value: 200, relative: 'absolute' },
          string_align: 'RIGHT',
        }),
      ]),
      measure(40, 12),
    );
    const box = layout.boxes[0];
    expect(box.x).toBe(160);
    expect(box.text?.runs[0].x).toBe(160);
  });

  it('treats Center as CenterTop and CenterCenter as vertical middle', () => {
    const top = layoutSkin(
      model([
        stringMeter({
          text: 'hello',
          y: { value: 10, relative: 'absolute' },
          string_align: 'CENTER',
        }),
      ]),
      measure(40, 12),
    ).boxes[0];
    const middle = layoutSkin(
      model([
        stringMeter({
          text: 'hello',
          y: { value: 10, relative: 'absolute' },
          string_align: 'CENTERCENTER',
        }),
      ]),
      measure(40, 12),
    ).boxes[0];

    expect(top.y).toBe(10);
    expect(top.text?.y).toBe(10);
    expect(middle.y).toBe(4);
    expect(middle.text?.y).toBe(10);
  });

  it('applies padding to content', () => {
    const layout = layoutSkin(
      model([stringMeter({ padding: [4, 2, 6, 8] })]),
      measure(30, 10),
    );
    const box = layout.boxes[0];
    expect(box.width).toBe(40);
    expect(box.height).toBe(20);
    expect(box.content.x).toBe(4);
    expect(box.content.y).toBe(2);
    expect(box.content.width).toBe(30);
    expect(box.content.height).toBe(10);
  });

  it('honours explicit width and height', () => {
    const layout = layoutSkin(
      model([stringMeter({ width: 200, height: 50 })]),
      measure(10, 10),
    );
    expect(layout.boxes[0].width).toBe(200);
    expect(layout.boxes[0].height).toBe(50);
  });

  it('collapses hidden meters', () => {
    const layout = layoutSkin(
      model([stringMeter({ hidden: true })]),
      measure(30, 10),
    );
    expect(layout.boxes[0].width).toBe(0);
    expect(layout.boxes[0].height).toBe(0);
  });

  it('derives image height from its width and aspect ratio', () => {
    const layout = layoutSkin(
      model([imageMeter({ width: 80 })]),
      measure(0, 0),
      () => ({ width: 160, height: 40 }),
    );
    expect(layout.boxes[0].width).toBe(80);
    expect(layout.boxes[0].height).toBe(20);
  });

  it('uses image dimensions when available', () => {
    const layout = layoutSkin(model([imageMeter()]), measure(0, 0), () => ({
      width: 64,
      height: 32,
    }));
    expect(layout.boxes[0].width).toBe(64);
    expect(layout.boxes[0].height).toBe(32);
  });

  it('defaults bar size', () => {
    const layout = layoutSkin(model([barMeter()]), measure(0, 0));
    expect(layout.boxes[0].width).toBe(100);
    expect(layout.boxes[0].height).toBe(10);
  });

  it('prefers explicit skin dimensions', () => {
    const skin = model([stringMeter()]);
    skin.width = 500;
    skin.height = 300;
    const layout = layoutSkin(skin, measure(10, 10));
    expect(layout.width).toBe(500);
    expect(layout.height).toBe(300);
  });
});

describe('resolveText', () => {
  it('substitutes measure references', () => {
    const meter = stringMeter({
      text: 'CPU : %1%',
      measure_names: ['CPU'],
    });
    expect(resolveText(meter, { cpu: '42' })).toBe('CPU : 42%');
  });

  it('falls back to the first measure without Text', () => {
    const meter = stringMeter({ measure_names: ['CPU'] });
    expect(resolveText(meter, { CPU: '7' })).toBe('7');
  });

  it('applies case and affixes', () => {
    const meter = stringMeter({
      text: 'hello',
      string_case: 'UPPER',
      prefix: '[',
      postfix: ']',
    });
    expect(resolveText(meter, {})).toBe('[HELLO]');
  });
});

describe('hitTest', () => {
  function boxes() {
    const skin = model([
      stringMeter({
        name: 'Bottom',
        x: { value: 0, relative: 'absolute' },
        y: { value: 0, relative: 'absolute' },
      }),
      stringMeter({
        name: 'Top',
        x: { value: 0, relative: 'absolute' },
        y: { value: 0, relative: 'absolute' },
      }),
    ]);
    return layoutSkin(skin, measure(10, 10)).boxes;
  }

  it('returns the topmost box under the cursor', () => {
    expect(hitTest(boxes(), 5, 5)?.meter.name).toBe('Top');
  });

  it('returns null outside every box', () => {
    expect(hitTest(boxes(), 100, 100)).toBeNull();
  });

  it('skips boxes rejected by an alpha hit test', () => {
    expect(hitTest(boxes(), 5, 5, () => false)).toBeNull();
    expect(hitTest(boxes(), 5, 5, () => true)?.meter.name).toBe('Top');
  });

  it('clips children to their container and hides them outside', () => {
    const skin = model([
      stringMeter({
        name: 'Frame',
        width: 20,
        height: 20,
        text: 'x',
      }),
      stringMeter({
        name: 'Child',
        x: { value: 10, relative: 'absolute' },
        y: { value: 10, relative: 'absolute' },
        width: 20,
        height: 20,
        text: 'y',
        container: 'Frame',
      }),
      stringMeter({
        name: 'Far',
        x: { value: 60, relative: 'absolute' },
        y: { value: 60, relative: 'absolute' },
        width: 20,
        height: 20,
        text: 'z',
        container: 'Frame',
      }),
    ]);
    const boxes = layoutSkin(skin, measure(10, 10)).boxes;
    expect(boxes[1].clip).toEqual({ x: 10, y: 10, width: 10, height: 10 });
    expect(boxes[2].clip?.width).toBe(0);
    expect(hitTest(boxes, 15, 15)?.meter.name).toBe('Child');
    expect(hitTest(boxes, 65, 65)).toBeNull();
  });

  it('clips nested containers recursively', () => {
    const skin = model([
      stringMeter({ name: 'Outer', width: 40, height: 40, text: 'o' }),
      stringMeter({
        name: 'Middle',
        width: 30,
        height: 30,
        text: 'm',
        container: 'Outer',
      }),
      stringMeter({
        name: 'Inner',
        x: { value: 20, relative: 'absolute' },
        y: { value: 20, relative: 'absolute' },
        width: 20,
        height: 20,
        text: 'i',
        container: 'Middle',
      }),
    ]);
    const boxes = layoutSkin(skin, measure(10, 10)).boxes;
    expect(boxes[2].clip).toEqual({ x: 20, y: 20, width: 10, height: 10 });
  });

  it('breaks container cycles', () => {
    const skin = model([
      stringMeter({ name: 'A', width: 20, height: 20, container: 'B' }),
      stringMeter({ name: 'B', width: 20, height: 20, container: 'A' }),
    ]);
    const boxes = layoutSkin(skin, measure(10, 10)).boxes;
    expect(boxes[0].clip?.width).toBe(0);
    expect(boxes[1].clip?.width).toBe(0);
  });
});
