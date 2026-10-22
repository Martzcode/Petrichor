import type { MouseActionState } from '../shared/ipc';
import type { RenderMeter } from '../renderer/skin-model';

export type MouseStates = Map<string, Map<string, MouseActionState>>;

export function normalizeAction(action: string): string {
  const value = action.trim().toLocaleLowerCase();
  if (value === '*') return value;
  return value.endsWith('action') ? value : `${value}action`;
}

export function buttonAction(button: number, phase: string): string | null {
  const prefix =
    button === 0
      ? 'left'
      : button === 1
        ? 'middle'
        : button === 2
          ? 'right'
          : button === 3
            ? 'x1'
            : button === 4
              ? 'x2'
              : null;
  if (!prefix) return null;
  return `${prefix}mouse${phase}action`;
}

export function expandMouseVariables(
  command: string,
  x: number,
  y: number,
): string {
  const mouseX = String(Math.round(x));
  const mouseY = String(Math.round(y));
  return command
    .replace(/\$MouseX\$/gi, mouseX)
    .replace(/\[\$MouseX\]/gi, mouseX)
    .replace(/\$MouseY\$/gi, mouseY)
    .replace(/\[\$MouseY\]/gi, mouseY);
}

export function cursorName(name: string | null): string {
  switch ((name ?? '').trim().toLowerCase()) {
    case 'hand':
      return 'pointer';
    case 'text':
      return 'text';
    case 'help':
      return 'help';
    case 'busy':
      return 'progress';
    case 'cross':
    case 'pen':
      return 'crosshair';
    case 'no':
      return 'not-allowed';
    case 'sizeall':
      return 'move';
    case 'sizenesw':
      return 'nesw-resize';
    case 'sizens':
      return 'ns-resize';
    case 'sizenwse':
      return 'nwse-resize';
    case 'sizewe':
      return 'ew-resize';
    case 'uparrow':
      return 'n-resize';
    case 'wait':
      return 'wait';
    case 'arrow':
    case 'custom':
    default:
      return 'pointer';
  }
}

export function isActionSuppressed(
  meter: Pick<RenderMeter, 'name' | 'group'>,
  action: string,
  statesByTarget: MouseStates,
): boolean {
  const meterName = meter.name.toLocaleLowerCase();
  for (const [target, states] of statesByTarget) {
    const matchesTarget =
      target === '*' ||
      target === meterName ||
      (target.startsWith('group:') &&
        (target === 'group:*' ||
          meter.group.some(
            (group) => group.toLocaleLowerCase() === target.slice(6),
          )));
    if (!matchesTarget) continue;
    for (const [name, state] of states) {
      if (state === 'enabled') continue;
      if (
        name === '*' ||
        name.split('|').map(normalizeAction).includes(normalizeAction(action))
      ) {
        return true;
      }
    }
  }
  return false;
}
