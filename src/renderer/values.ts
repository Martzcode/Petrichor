import {
  HistogramMeter,
  LineMeter,
  MeasureValue,
  RenderMeter,
  RenderModel,
} from './skin-model';

export type MeterHistory = number[][];
export type MeterHistories = Map<string, MeterHistory>;

const MAX_HISTORY = 8192;

export function lookupMeasure(
  values: Record<string, MeasureValue>,
  name: string | null | undefined,
): MeasureValue | null {
  if (!name) return null;
  const key = Object.keys(values).find(
    (candidate) => candidate.toLowerCase() === name.toLowerCase(),
  );
  return key ? values[key] : null;
}

export function measureRatio(value: MeasureValue | null): number {
  if (!value) return 0;
  const min = value.min ?? 0;
  const max = value.max ?? 100;
  const span = max - min;
  if (Math.abs(span) < Number.EPSILON) return 0;
  return Math.min(Math.max((value.number - min) / span, 0), 1);
}

export function updateMeterValues(
  model: RenderModel,
  values: Record<string, MeasureValue>,
): void {
  for (const meter of model.meters) {
    switch (meter.type) {
      case 'bar':
        meter.value = measureRatio(
          lookupMeasure(values, meter.measure_names[0]),
        );
        break;
      case 'histogram':
        updateHistogram(meter, values);
        break;
      case 'line':
        updateLine(meter, values);
        break;
      default:
        break;
    }
  }
}

function updateHistogram(
  meter: HistogramMeter,
  values: Record<string, MeasureValue>,
): void {
  const primary = meter.primary_measure ?? meter.measure_names[0];
  const secondary = meter.secondary_measure ?? meter.measure_names[1];
  meter.primary = measureRatio(lookupMeasure(values, primary));
  meter.secondary = measureRatio(lookupMeasure(values, secondary));
}

function updateLine(
  meter: LineMeter,
  values: Record<string, MeasureValue>,
): void {
  meter.values = Array.from({ length: meter.line_count }, (_, index) =>
    measureRatio(lookupMeasure(values, meter.measure_names[index])),
  );
}

export function resetHistories(histories: MeterHistories): void {
  histories.clear();
}

export function appendHistories(
  histories: MeterHistories,
  model: RenderModel,
): void {
  for (const meter of model.meters) {
    const sample = sampleOf(meter);
    if (!sample) continue;
    let history = histories.get(meter.name);
    if (!history) {
      history = sample.map(() => []);
      histories.set(meter.name, history);
    }
    while (history.length < sample.length) history.push([]);
    for (let index = 0; index < sample.length; index += 1) {
      const channel = history[index];
      channel.push(sample[index]);
      if (channel.length > MAX_HISTORY) {
        channel.splice(0, channel.length - MAX_HISTORY);
      }
    }
  }
}

function sampleOf(meter: RenderMeter): number[] | null {
  if (meter.type === 'histogram') {
    return [meter.primary, meter.secondary];
  }
  if (meter.type === 'line') {
    return meter.values.slice();
  }
  return null;
}
