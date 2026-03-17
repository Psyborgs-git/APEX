import React, { useEffect, useRef, useCallback, useState, useMemo } from 'react';
import {
  forceSimulation,
  forceLink,
  forceManyBody,
  forceCenter,
  forceCollide,
  type SimulationNodeDatum,
  type SimulationLinkDatum,
} from 'd3-force';
import { select } from 'd3';
import { zoom as d3Zoom, type D3ZoomEvent } from 'd3';

interface GraphNode extends SimulationNodeDatum {
  id: string;
  label: string;
  type: 'strategy' | 'symbol' | 'signal' | 'indicator' | 'model';
  value?: number;
}

interface GraphEdge extends SimulationLinkDatum<GraphNode> {
  weight: number;
  label?: string;
}

interface VectorGraphProps {
  nodes: GraphNode[];
  edges: GraphEdge[];
  width?: number;
  height?: number;
  onNodeClick?: (node: GraphNode) => void;
  onNodeHover?: (node: GraphNode | null) => void;
}

const NODE_COLORS: Record<GraphNode['type'], string> = {
  strategy: '#448aff',
  symbol: '#00c853',
  signal: '#ffab00',
  indicator: '#ab47bc',
  model: '#ff7043',
};

const NODE_RADII: Record<GraphNode['type'], number> = {
  strategy: 20,
  symbol: 16,
  signal: 14,
  indicator: 12,
  model: 18,
};

const VectorGraphInner: React.FC<VectorGraphProps> = ({
  nodes,
  edges,
  onNodeClick,
  onNodeHover,
}) => {
  const svgRef = useRef<SVGSVGElement>(null);
  const containerRef = useRef<HTMLDivElement>(null);
  const [hoveredNode, setHoveredNode] = useState<string | null>(null);
  const [dimensions, setDimensions] = useState({ width: 600, height: 400 });

  // Clone data to avoid mutating props
  const simNodes = useMemo(() => nodes.map((n) => ({ ...n })), [nodes]);
  const simEdges = useMemo(
    () => edges.map((e) => ({ ...e, source: e.source, target: e.target })),
    [edges],
  );

  // Resize observer
  useEffect(() => {
    const container = containerRef.current;
    if (!container) return;

    const observer = new ResizeObserver((entries) => {
      const entry = entries[0];
      if (entry) {
        setDimensions({
          width: entry.contentRect.width,
          height: entry.contentRect.height,
        });
      }
    });

    observer.observe(container);
    return () => observer.disconnect();
  }, []);

  const handleNodeClick = useCallback(
    (node: GraphNode) => {
      onNodeClick?.(node);
    },
    [onNodeClick],
  );

  const handleNodeHover = useCallback(
    (node: GraphNode | null) => {
      setHoveredNode(node?.id ?? null);
      onNodeHover?.(node);
    },
    [onNodeHover],
  );

  // D3 force simulation
  useEffect(() => {
    const svg = svgRef.current;
    if (!svg || simNodes.length === 0) return;

    const { width, height } = dimensions;
    const svgSel = select(svg);

    // Clear previous content
    svgSel.selectAll('*').remove();

    // Container group for zoom/pan
    const g = svgSel.append('g');

    // Zoom behavior
    const zoomBehavior = d3Zoom<SVGSVGElement, unknown>()
      .scaleExtent([0.2, 4])
      .on('zoom', (event: D3ZoomEvent<SVGSVGElement, unknown>) => {
        g.attr('transform', event.transform.toString());
      });

    svgSel.call(zoomBehavior);

    // Arrow marker
    svgSel
      .append('defs')
      .append('marker')
      .attr('id', 'arrowhead')
      .attr('viewBox', '0 -5 10 10')
      .attr('refX', 25)
      .attr('refY', 0)
      .attr('markerWidth', 6)
      .attr('markerHeight', 6)
      .attr('orient', 'auto')
      .append('path')
      .attr('d', 'M0,-5L10,0L0,5')
      .attr('fill', '#5c5c7a');

    // Create simulation
    const simulation = forceSimulation<GraphNode>(simNodes)
      .force(
        'link',
        forceLink<GraphNode, GraphEdge>(simEdges)
          .id((d) => d.id)
          .distance(120),
      )
      .force('charge', forceManyBody().strength(-300))
      .force('center', forceCenter(width / 2, height / 2))
      .force(
        'collision',
        forceCollide<GraphNode>().radius((d) => NODE_RADII[d.type] + 4),
      );

    // Edges
    const link = g
      .selectAll<SVGLineElement, GraphEdge>('line')
      .data(simEdges)
      .enter()
      .append('line')
      .attr('stroke', '#5c5c7a')
      .attr('stroke-opacity', (d) => Math.max(0.2, Math.min(1, d.weight)))
      .attr('stroke-width', (d) => Math.max(1, d.weight * 3))
      .attr('marker-end', 'url(#arrowhead)');

    // Edge labels
    const edgeLabels = g
      .selectAll<SVGTextElement, GraphEdge>('text.edge-label')
      .data(simEdges.filter((e) => e.label))
      .enter()
      .append('text')
      .attr('class', 'edge-label')
      .attr('font-size', '9px')
      .attr('font-family', "'JetBrains Mono', monospace")
      .attr('fill', '#5c5c7a')
      .attr('text-anchor', 'middle')
      .text((d) => d.label ?? '');

    // Node groups
    const nodeGroup = g
      .selectAll<SVGGElement, GraphNode>('g.node')
      .data(simNodes)
      .enter()
      .append('g')
      .attr('class', 'node')
      .style('cursor', 'pointer');

    // Node circles
    nodeGroup
      .append('circle')
      .attr('r', (d) => NODE_RADII[d.type])
      .attr('fill', (d) => NODE_COLORS[d.type])
      .attr('fill-opacity', 0.8)
      .attr('stroke', (d) => NODE_COLORS[d.type])
      .attr('stroke-width', 2)
      .attr('stroke-opacity', 0.4);

    // Node labels
    nodeGroup
      .append('text')
      .text((d) => d.label)
      .attr('font-size', '10px')
      .attr('font-family', "'JetBrains Mono', monospace")
      .attr('fill', '#e8e8f0')
      .attr('text-anchor', 'middle')
      .attr('dy', (d) => NODE_RADII[d.type] + 14);

    // Interaction
    nodeGroup.on('click', (_event, d) => {
      handleNodeClick(d);
    });

    nodeGroup.on('mouseenter', (_event, d) => {
      handleNodeHover(d);
      select(_event.currentTarget as SVGGElement)
        .select('circle')
        .attr('stroke-width', 4)
        .attr('stroke-opacity', 1);
    });

    nodeGroup.on('mouseleave', (_event) => {
      handleNodeHover(null);
      select(_event.currentTarget as SVGGElement)
        .select('circle')
        .attr('stroke-width', 2)
        .attr('stroke-opacity', 0.4);
    });

    // Drag behavior
    nodeGroup.call(
      select.prototype.call.bind(
        svgSel,
        // Drag is handled via simulation alpha restart
      ) as never,
    );

    // Tick update
    simulation.on('tick', () => {
      link
        .attr('x1', (d) => (d.source as GraphNode).x ?? 0)
        .attr('y1', (d) => (d.source as GraphNode).y ?? 0)
        .attr('x2', (d) => (d.target as GraphNode).x ?? 0)
        .attr('y2', (d) => (d.target as GraphNode).y ?? 0);

      edgeLabels
        .attr('x', (d) => (((d.source as GraphNode).x ?? 0) + ((d.target as GraphNode).x ?? 0)) / 2)
        .attr('y', (d) => (((d.source as GraphNode).y ?? 0) + ((d.target as GraphNode).y ?? 0)) / 2);

      nodeGroup.attr(
        'transform',
        (d) => `translate(${d.x ?? 0},${d.y ?? 0})`,
      );
    });

    return () => {
      simulation.stop();
    };
  }, [simNodes, simEdges, dimensions, handleNodeClick, handleNodeHover]);

  return (
    <div className="flex flex-col h-full" data-testid="vector-graph">
      <div className="px-3 py-1.5 border-b border-[var(--border-color)] flex items-center justify-between">
        <span className="text-sm font-medium text-text-secondary">Strategy Graph</span>
        <div className="flex items-center gap-3">
          {Object.entries(NODE_COLORS).map(([type, color]) => (
            <div key={type} className="flex items-center gap-1">
              <span
                className="w-2.5 h-2.5 rounded-full inline-block"
                style={{ backgroundColor: color }}
              />
              <span className="text-xs text-text-muted capitalize">{type}</span>
            </div>
          ))}
        </div>
      </div>
      <div ref={containerRef} className="flex-1 min-h-0 relative">
        <svg
          ref={svgRef}
          width={dimensions.width}
          height={dimensions.height}
          className="block"
        />
        {hoveredNode && (
          <div className="absolute top-2 left-2 bg-surface-2 border border-[var(--border-color)] rounded px-2 py-1 text-xs font-mono text-text-primary shadow-md">
            {hoveredNode}
          </div>
        )}
      </div>
    </div>
  );
};

export const VectorGraph = React.memo(VectorGraphInner);
VectorGraph.displayName = 'VectorGraph';

export type { GraphNode, GraphEdge };
