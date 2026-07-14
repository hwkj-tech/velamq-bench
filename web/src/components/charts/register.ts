import { use } from 'echarts/core';
import { BarChart, HeatmapChart, LineChart } from 'echarts/charts';
import {
  DataZoomComponent,
  GraphicComponent,
  GridComponent,
  LegendComponent,
  MarkAreaComponent,
  MarkLineComponent,
  TitleComponent,
  TooltipComponent,
} from 'echarts/components';
import { CanvasRenderer } from 'echarts/renderers';

use([
  BarChart,
  CanvasRenderer,
  DataZoomComponent,
  GraphicComponent,
  GridComponent,
  HeatmapChart,
  LegendComponent,
  LineChart,
  MarkAreaComponent,
  MarkLineComponent,
  TitleComponent,
  TooltipComponent,
]);
