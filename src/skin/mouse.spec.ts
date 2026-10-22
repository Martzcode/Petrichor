import { describe, expect, it } from 'vitest';
import {
  buttonAction,
  cursorName,
  expandMouseVariables,
  isActionSuppressed,
  normalizeAction,
} from './mouse';
import type { MouseActionState } from '../shared/ipc';

function states(
  entries: Array<[string, string, MouseActionState]>,
): Map<string, Map<string, MouseActionState>> {
  const result = new Map<string, Map<string, MouseActionState>>();
  for (const [target, action, state] of entries) {
    const values = result.get(target) ?? new Map();
    values.set(normalizeAction(action), state);
    result.set(target, values);
  }
  return result;
}

describe('mouse actions', () => {
  it('maps all supported buttons', () => {
    expect(buttonAction(0, 'down')).toBe('leftmousedownaction');
    expect(buttonAction(1, 'up')).toBe('middlemouseupaction');
    expect(buttonAction(2, 'doubleclick')).toBe('rightmousedoubleclickaction');
    expect(buttonAction(3, 'up')).toBe('x1mouseupaction');
    expect(buttonAction(4, 'up')).toBe('x2mouseupaction');
    expect(buttonAction(9, 'up')).toBeNull();
  });

  it('normalizes existing and short action names', () => {
    expect(normalizeAction('LeftMouseUp')).toBe('leftmouseupaction');
    expect(normalizeAction('LeftMouseUpAction')).toBe('leftmouseupaction');
    expect(normalizeAction(' * ')).toBe('*');
  });

  it('supports wildcard, meter and group suppression', () => {
    const meter = { name: 'Clock', group: ['Top'] };
    expect(
      isActionSuppressed(
        meter,
        'LeftMouseUpAction',
        states([['clock', 'LeftMouseUpAction', 'disabled']]),
      ),
    ).toBe(true);
    expect(
      isActionSuppressed(
        meter,
        'RightMouseUpAction',
        states([['group:top', 'RightMouseUp', 'cleared']]),
      ),
    ).toBe(true);
    expect(
      isActionSuppressed(
        meter,
        'LeftMouseUpAction',
        states([['*', '*', 'disabled']]),
      ),
    ).toBe(true);
    expect(
      isActionSuppressed(
        meter,
        'LeftMouseUpAction',
        states([['other', 'LeftMouseUpAction', 'disabled']]),
      ),
    ).toBe(false);
  });

  it('re-enables actions when the state is removed', () => {
    const meter = { name: 'Clock', group: [] };
    expect(
      isActionSuppressed(
        meter,
        'LeftMouseUpAction',
        states([['clock', 'LeftMouseUpAction', 'enabled']]),
      ),
    ).toBe(false);
  });

  it('expands mouse coordinates', () => {
    expect(expandMouseVariables('!Log "$MouseX$,$MouseY$"', 12.4, 8.6)).toBe(
      '!Log "12,9"',
    );
    expect(expandMouseVariables('[$MouseX];[$MouseY]', 5, 7)).toBe('5;7');
  });

  it('maps Rainmeter cursors to CSS cursors', () => {
    expect(cursorName('Hand')).toBe('pointer');
    expect(cursorName('SizeNESW')).toBe('nesw-resize');
    expect(cursorName('No')).toBe('not-allowed');
    expect(cursorName(null)).toBe('pointer');
  });
});
