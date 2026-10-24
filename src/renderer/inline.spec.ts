import { buildTextRuns, parseColor, POINTS_TO_PIXELS } from './inline';
import { StringMeter } from './skin-model';

function meter(overrides: Partial<StringMeter> = {}): StringMeter {
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
    text: 'Hello 42',
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

describe('buildTextRuns', () => {
  it('splits text and applies a color to the match', () => {
    const runs = buildTextRuns(
      meter({
        inline_settings: ['COLOR FF0000FF'],
        inline_patterns: ['\\d+'],
      }),
      'Hello 42',
    );
    expect(runs.map((run) => run.text)).toEqual(['Hello ', '42']);
    expect(runs[1].style.color).toEqual({ r: 255, g: 0, b: 0, a: 255 });
  });

  it('uses capture groups when present', () => {
    const runs = buildTextRuns(
      meter({
        inline_settings: ['SIZE 20'],
        inline_patterns: ['CPU: (\\d+)%'],
      }),
      'CPU: 42%',
    );
    expect(runs.map((run) => run.text)).toEqual(['CPU: ', '42', '%']);
    expect(runs[1].style.size).toBe(20 * POINTS_TO_PIXELS);
    expect(runs[0].style.size).toBe(12 * POINTS_TO_PIXELS);
    expect(runs[2].style.size).toBe(12 * POINTS_TO_PIXELS);
  });

  it('applies case and merges styles', () => {
    const runs = buildTextRuns(
      meter({
        inline_settings: ['CASE Upper', 'WEIGHT 800', 'UNDERLINE'],
        inline_patterns: ['hello', 'hello', 'hello'],
      }),
      'hello world',
    );
    expect(runs[0].text).toBe('HELLO');
    expect(runs[0].style.weight).toBe(800);
    expect(runs[0].style.underline).toBe(true);
    expect(runs[1].text).toBe(' world');
  });

  it('applies a setting without a pattern to the whole string', () => {
    const runs = buildTextRuns(
      meter({ inline_settings: ['CharacterSpacing | 10 | 10'] }),
      'Hello',
    );
    expect(runs).toHaveLength(1);
    expect(runs[0].style.spacing).toEqual({
      leading: 10,
      trailing: 10,
      advance: null,
    });
  });

  it('ignores invalid settings and patterns', () => {
    const runs = buildTextRuns(
      meter({
        inline_settings: ['NOPE', 'COLOR pasuneCouleur'],
        inline_patterns: ['x', 'y'],
      }),
      'Hello 42',
    );
    expect(runs.length).toBe(1);
    expect(runs[0].text).toBe('Hello 42');
  });
});

describe('parseColor', () => {
  it('parses RRGGBB and RRGGBBAA', () => {
    expect(parseColor('38BDF8')).toEqual({ r: 56, g: 189, b: 248, a: 255 });
    expect(parseColor('#38BDF880')).toEqual({
      r: 56,
      g: 189,
      b: 248,
      a: 128,
    });
    expect(parseColor('nope')).toBeNull();
  });
});
