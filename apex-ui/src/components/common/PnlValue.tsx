import React from 'react';
import { formatPnl, formatPct } from '../../lib/format';

interface PnlValueProps {
  value: number;
  showSign?: boolean;
  className?: string;
  type?: 'currency' | 'percent';
}

export const PnlValue: React.FC<PnlValueProps> = React.memo(({
  value,
  className = '',
  type = 'currency',
}) => {
  const color = value > 0 ? 'text-bull' : value < 0 ? 'text-bear' : 'text-text-muted';
  const formatted = type === 'percent' ? formatPct(value) : formatPnl(value);

  return (
    <span className={`font-mono ${color} ${className}`} data-numeric>
      {formatted}
    </span>
  );
});

PnlValue.displayName = 'PnlValue';
