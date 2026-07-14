import type { Annotation, MetricSnapshot } from '@/api/types';

export const chartColors = ['#24c8ff', '#17e7b5', '#8b7cff', '#ffb63d', '#ff5277', '#4f7cff', '#7be7ff'];

export function xOf(snapshot: MetricSnapshot) {
  return snapshot.elapsed_ms / 1000;
}

export function chartGrid(top = 24, bottom = 60) {
  return { left: 42, right: 18, top, bottom, containLabel: true };
}

export function chartLegend() {
  return {
    type: 'scroll',
    bottom: 0,
    left: 'center',
    width: '80%',
    icon: 'circle',
    itemWidth: 8,
    itemHeight: 8,
    itemGap: 18,
    textStyle: { color: '#89aabd', fontWeight: 700, fontSize: 12 },
    pageIconColor: '#24c8ff',
    pageIconInactiveColor: '#355064',
    pageTextStyle: { color: '#7897aa' },
  };
}

export function chartTooltip() {
  return {
    trigger: 'axis',
    confine: true,
    formatter: formatTooltip,
    backgroundColor: 'rgba(3, 14, 25, 0.96)',
    borderColor: 'rgba(50, 184, 255, 0.45)',
    borderWidth: 1,
    textStyle: { color: '#e9f7ff' },
    axisPointer: { type: 'cross', lineStyle: { color: 'rgba(50,184,255,.65)', width: 1, type: 'dashed' } },
    extraCssText: 'box-shadow: 0 20px 54px rgba(0,0,0,.55), 0 0 24px rgba(36,200,255,.10); border-radius: 10px; backdrop-filter: blur(14px);',
  };
}

export function timeAxis(values: number[] = []) {
  const max = niceTimeMax(values);
  return {
    type: 'value',
    name: 's',
    nameLocation: 'end',
    nameGap: 10,
    min: 0,
    max,
    minInterval: max <= 10 ? 1 : undefined,
    splitNumber: max <= 10 ? 5 : 6,
    axisLabel: { color: '#6f93a8', margin: 10, formatter: formatAxisNumber },
    axisLine: { lineStyle: { color: 'rgba(70,151,187,.35)' } },
    axisTick: { show: false },
    splitLine: { lineStyle: { color: 'rgba(74,151,184,.12)', type: 'dashed' } },
  };
}

export function valueAxis(values: number[], name?: string, zeroMax = 1) {
  const max = niceMax(values, zeroMax);
  return {
    type: 'value',
    name,
    min: 0,
    max,
    splitNumber: 4,
    axisLabel: { color: '#6f93a8', margin: 8, formatter: formatAxisNumber },
    axisLine: { lineStyle: { color: 'rgba(70,151,187,.35)' } },
    axisTick: { show: false },
    splitLine: { lineStyle: { color: 'rgba(74,151,184,.12)', type: 'dashed' } },
  };
}

export function chartDataZoom() {
  return [];
}

export function lineSeriesBase(index: number, pointCount = 0) {
  const color = chartColors[index % chartColors.length];
  return {
    smooth: true,
    showSymbol: pointCount > 0 && pointCount <= 24,
    symbol: 'circle',
    symbolSize: 7,
    lineStyle: { width: 2.6, cap: 'round', join: 'round', color, shadowColor: color, shadowBlur: 10 },
    itemStyle: { color, borderColor: '#dff9ff', borderWidth: 1, shadowColor: color, shadowBlur: 12 },
    areaStyle: {
      opacity: 1,
      color: {
        type: 'linear',
        x: 0,
        y: 0,
        x2: 0,
        y2: 1,
        colorStops: [
          { offset: 0, color: `${color}33` },
          { offset: 1, color: `${color}00` },
        ],
      },
    },
    emphasis: { focus: 'series', scale: 1.3 },
    color,
  };
}

export function niceMax(values: number[], zeroMax = 1) {
  const raw = Math.max(...values.filter(Number.isFinite), 0);
  if (raw <= 0) return zeroMax;
  const pow = 10 ** Math.floor(Math.log10(raw));
  const scaled = raw / pow;
  const nice = scaled <= 1 ? 1 : scaled <= 2 ? 2 : scaled <= 5 ? 5 : 10;
  return nice * pow;
}

export function niceTimeMax(values: number[]) {
  const raw = Math.max(...values.filter(Number.isFinite), 0);
  if (raw <= 10) return 10;
  if (raw <= 60) return 60;
  return niceMax(values);
}

export function hasVisibleSignal(values: number[]) {
  return values.some((value) => Number.isFinite(value) && Math.abs(value) > 0.000001);
}

export function emptyChartGraphic(show: boolean, text = '暂无指标数据') {
  if (!show) return undefined;
  return {
    type: 'text',
    left: 'center',
    top: 'middle',
    silent: true,
    style: {
      text,
      fill: '#6f93a8',
      fontSize: 14,
      fontWeight: 700,
    },
  };
}

function formatAxisNumber(value: number) {
  if (Math.abs(value) >= 1000) return `${Math.round(value / 1000)}k`;
  if (Math.abs(value) >= 10 || Number.isInteger(value)) return String(Math.round(value));
  if (Math.abs(value) >= 1) return value.toFixed(1);
  return value.toFixed(2).replace(/0+$/, '').replace(/\.$/, '');
}

function formatMetricNumber(value: unknown) {
  const numeric = Number(value);
  if (!Number.isFinite(numeric)) return String(value ?? '');
  if (Math.abs(numeric) >= 1000) return numeric.toLocaleString(undefined, { maximumFractionDigits: 0 });
  if (Math.abs(numeric) >= 100) return numeric.toFixed(1).replace(/\.0$/, '');
  if (Math.abs(numeric) >= 10) return numeric.toFixed(2).replace(/0+$/, '').replace(/\.$/, '');
  if (Math.abs(numeric) >= 1) return numeric.toFixed(2).replace(/0+$/, '').replace(/\.$/, '');
  return numeric.toFixed(3).replace(/0+$/, '').replace(/\.$/, '');
}

type TooltipParam = {
  axisValue?: unknown;
  marker?: string;
  name?: string;
  seriesName?: string;
  value?: unknown;
};

function formatTooltip(params: unknown) {
  const rows = (Array.isArray(params) ? params : [params]) as TooltipParam[];
  const axisValue = rows[0]?.axisValue ?? rows[0]?.name ?? '';
  const header = `${formatMetricNumber(axisValue)}s`;
  const body = rows
    .map((row) => {
      const value = Array.isArray(row.value) ? row.value[row.value.length - 1] : row.value;
      return `<div style="display:flex;align-items:center;gap:8px;min-width:190px;margin-top:6px;">${row.marker ?? ''}<span style="flex:1;">${escapeHtml(row.seriesName ?? '')}</span><strong style="font-variant-numeric:tabular-nums;">${formatMetricNumber(value)}</strong></div>`;
    })
    .join('');
  return `<div><div style="font-weight:700;margin-bottom:4px;">${escapeHtml(header)}</div>${body}</div>`;
}

function escapeHtml(value: string) {
  return value.replace(/[&<>"']/g, (char) => {
    if (char === '&') return '&amp;';
    if (char === '<') return '&lt;';
    if (char === '>') return '&gt;';
    if (char === '"') return '&quot;';
    return '&#39;';
  });
}

export function shortSeriesName(id: string) {
  if (id.length <= 18) return id;
  return `${id.slice(0, 4)}…${id.slice(-3)}`;
}

export function annotationMarkLines(annotations: Annotation[]) {
  return annotations.map((annotation) => ({
    xAxis: Math.max(0, (new Date(annotation.ts).getTime() - new Date(annotations[0]?.ts ?? annotation.ts).getTime()) / 1000),
    label: { formatter: annotation.title },
    lineStyle: { color: colorByCategory(annotation.category), type: 'dashed' },
  }));
}

export function colorByCategory(category: string) {
  if (category === 'sla_breach') return '#dc3f59';
  if (category === 'broker_event') return '#0891b2';
  if (category === 'config_change') return '#f59e0b';
  return '#64748b';
}

export function groupedSnapshots(snapshots: MetricSnapshot[]) {
  return snapshots.reduce<Record<string, MetricSnapshot[]>>((groups, snapshot) => {
    const key = snapshot.run_workload_id ?? 'aggregate';
    groups[key] = [...(groups[key] ?? []), snapshot];
    return groups;
  }, {});
}
