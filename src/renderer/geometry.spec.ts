import { center, clamp, contains, rect } from './geometry';

describe('geometry', () => {
  const bounds = rect(10, 20, 100, 50);

  it('detects points inside a rect', () => {
    expect(contains(bounds, { x: 10, y: 20 })).toBe(true);
    expect(contains(bounds, { x: 110, y: 70 })).toBe(true);
    expect(contains(bounds, { x: 9, y: 20 })).toBe(false);
    expect(contains(bounds, { x: 111, y: 70 })).toBe(false);
  });

  it('computes the center', () => {
    expect(center(bounds)).toEqual({ x: 60, y: 45 });
  });

  it('clamps values', () => {
    expect(clamp(5, 0, 10)).toBe(5);
    expect(clamp(-1, 0, 10)).toBe(0);
    expect(clamp(42, 0, 10)).toBe(10);
  });
});
