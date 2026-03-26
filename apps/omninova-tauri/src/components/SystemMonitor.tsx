/**
 * System Monitor Component
 *
 * Displays real-time system resource usage (CPU, Memory, Disk)
 * with historical charts and export functionality.
 *
 * [Source: Story 9-1 - System Resource Monitor]
 */

import React, { useState } from 'react';
import { useSystemResources, useSystemHistory, useSystemExport } from '../hooks/useSystemMonitor';

// ============================================================================
// Styles
// ============================================================================

const styles = {
  container: {
    padding: '16px',
    display: 'flex',
    flexDirection: 'column' as const,
    gap: '16px',
  },
  card: {
    backgroundColor: '#1e1e2e',
    borderRadius: '8px',
    padding: '16px',
    border: '1px solid #333',
  },
  title: {
    fontSize: '14px',
    fontWeight: 600,
    color: '#cdd6f4',
    marginBottom: '12px',
    display: 'flex',
    alignItems: 'center',
    gap: '8px',
  },
  value: {
    fontSize: '32px',
    fontWeight: 700,
    color: '#89b4fa',
  },
  label: {
    fontSize: '12px',
    color: '#6c7086',
    marginTop: '4px',
  },
  warning: {
    color: '#f9e2af',
  },
  error: {
    color: '#f38ba8',
  },
  success: {
    color: '#a6e3a1',
  },
  grid: {
    display: 'grid',
    gridTemplateColumns: 'repeat(auto-fit, minmax(200px, 1fr))',
    gap: '16px',
  },
  progressBar: {
    width: '100%',
    height: '8px',
    backgroundColor: '#313244',
    borderRadius: '4px',
    overflow: 'hidden',
    marginTop: '8px',
  },
  progressFill: (percent: number, warning: boolean) => ({
    width: `${Math.min(percent, 100)}%`,
    height: '100%',
    backgroundColor: warning ? '#f9e2af' : '#89b4fa',
    transition: 'width 0.3s ease',
  }),
  chart: {
    width: '100%',
    height: '120px',
    backgroundColor: '#313244',
    borderRadius: '4px',
    marginTop: '8px',
  },
  button: {
    padding: '8px 16px',
    backgroundColor: '#89b4fa',
    color: '#1e1e2e',
    border: 'none',
    borderRadius: '4px',
    cursor: 'pointer',
    fontSize: '12px',
    fontWeight: 500,
  },
  buttonSecondary: {
    padding: '8px 16px',
    backgroundColor: '#313244',
    color: '#cdd6f4',
    border: '1px solid #45475a',
    borderRadius: '4px',
    cursor: 'pointer',
    fontSize: '12px',
    fontWeight: 500,
  },
  flexRow: {
    display: 'flex',
    gap: '8px',
    alignItems: 'center',
  },
  diskRow: {
    display: 'flex',
    justifyContent: 'space-between',
    alignItems: 'center',
    padding: '8px 0',
    borderBottom: '1px solid #313244',
  },
};

// ============================================================================
// Components
// ============================================================================

/**
 * Progress bar component
 */
function ProgressBar({ 
  percent, 
  warning = false, 
  label 
}: { 
  percent: number; 
  warning?: boolean;
  label?: string;
}) {
  return (
    <div>
      <div style={styles.flexRow}>
        <span style={styles.label}>{label}</span>
        <span style={{ ...styles.label, ...(warning ? styles.warning : {}) }}>
          {percent.toFixed(1)}%
        </span>
      </div>
      <div style={styles.progressBar}>
        <div style={styles.progressFill(percent, warning)} />
      </div>
    </div>
  );
}

/**
 * Mini chart component (simple SVG line chart)
 */
function MiniChart({ 
  data, 
  color = '#89b4fa',
  height = 120 
}: { 
  data: { timestamp: number; value: number }[];
  color?: string;
  height?: number;
}) {
  if (data.length < 2) {
    return (
      <div style={styles.chart}>
        <div style={{ 
          height: '100%', 
          display: 'flex', 
          alignItems: 'center', 
          justifyContent: 'center',
          color: '#6c7086',
          fontSize: '12px',
        }}>
          Collecting data...
        </div>
      </div>
    );
  }

  const width = 400;
  const padding = 10;
  const chartWidth = width - padding * 2;
  const chartHeight = height - padding * 2;

  const values = data.map(d => d.value);
  const min = Math.min(...values);
  const max = Math.max(...values);
  const range = max - min || 1;

  const points = data.map((d, i) => {
    const x = padding + (i / (data.length - 1)) * chartWidth;
    const y = padding + chartHeight - ((d.value - min) / range) * chartHeight;
    return `${x},${y}`;
  }).join(' ');

  return (
    <svg width="100%" height={height} viewBox={`0 0 ${width} ${height}`} style={styles.chart}>
      <polyline
        fill="none"
        stroke={color}
        strokeWidth="2"
        points={points}
      />
      {/* Show min/max labels */}
      <text x={width - padding} y={padding + 10} fill="#6c7086" fontSize="10" textAnchor="end">
        {max.toFixed(1)}
      </text>
      <text x={width - padding} y={height - padding} fill="#6c7086" fontSize="10" textAnchor="end">
        {min.toFixed(1)}
      </text>
    </svg>
  );
}

/**
 * Resource card component
 */
function ResourceCard({
  title,
  icon,
  children,
}: {
  title: string;
  icon: string;
  children: React.ReactNode;
}) {
  return (
    <div style={styles.card}>
      <div style={styles.title}>
        <span>{icon}</span>
        <span>{title}</span>
      </div>
      {children}
    </div>
  );
}

/**
 * Main System Monitor component
 */
export function SystemMonitor() {
  const { resources, loading, error } = useSystemResources(5000);
  const { history } = useSystemHistory(30000);
  const { exportData, exporting } = useSystemExport();
  const [exportFormat, setExportFormat] = useState<'json' | 'csv'>('json');

  if (loading) {
    return (
      <div style={styles.container}>
        <div style={styles.card}>
          <div style={{ color: '#6c7086', textAlign: 'center', padding: '20px' }}>
            Loading system resources...
          </div>
        </div>
      </div>
    );
  }

  if (error) {
    return (
      <div style={styles.container}>
        <div style={styles.card}>
          <div style={{ ...styles.error, textAlign: 'center', padding: '20px' }}>
            Error: {error}
          </div>
        </div>
      </div>
    );
  }

  if (!resources) {
    return null;
  }

  const memoryWarning = resources.memory.warning;

  return (
    <div style={styles.container}>
      {/* Header with export */}
      <div style={{ ...styles.card, display: 'flex', justifyContent: 'space-between', alignItems: 'center' }}>
        <div style={styles.title}>
          <span>📊</span>
          <span>System Monitor</span>
          {memoryWarning && (
            <span style={{ ...styles.warning, fontSize: '12px', marginLeft: '8px' }}>
              ⚠️ High Memory Usage
            </span>
          )}
        </div>
        <div style={styles.flexRow}>
          <select 
            value={exportFormat}
            onChange={(e) => setExportFormat(e.target.value as 'json' | 'csv')}
            style={{ ...styles.buttonSecondary, padding: '4px 8px' }}
          >
            <option value="json">JSON</option>
            <option value="csv">CSV</option>
          </select>
          <button 
            style={styles.button}
            onClick={() => exportData(exportFormat)}
            disabled={exporting}
          >
            {exporting ? 'Exporting...' : 'Export'}
          </button>
        </div>
      </div>

      {/* Main metrics */}
      <div style={styles.grid}>
        {/* CPU */}
        <ResourceCard title="CPU Usage" icon="🖥️">
          <div style={styles.value}>
            {resources.cpu.usage_percent.toFixed(1)}%
          </div>
          <ProgressBar percent={resources.cpu.usage_percent} />
          {history?.cpu && <MiniChart data={history.cpu} color="#89b4fa" />}
        </ResourceCard>

        {/* Memory */}
        <ResourceCard title="Memory Usage" icon="💾">
          <div style={{ ...styles.value, ...(memoryWarning ? styles.warning : {}) }}>
            {resources.memory.used_mb} MB
          </div>
          <div style={styles.label}>
            of {resources.memory.total_mb} MB
          </div>
          <ProgressBar 
            percent={resources.memory.usage_percent} 
            warning={memoryWarning}
          />
          {memoryWarning && (
            <div style={{ ...styles.warning, fontSize: '11px', marginTop: '8px' }}>
              ⚠️ Warning: Memory usage exceeds {resources.memory.warning_threshold_mb} MB
            </div>
          )}
          {history?.memory && <MiniChart data={history.memory} color={memoryWarning ? '#f9e2af' : '#a6e3a1'} />}
        </ResourceCard>
      </div>

      {/* Disk usage */}
      {resources.disks.length > 0 && (
        <ResourceCard title="Disk Usage" icon="💿">
          {resources.disks.map((disk, i) => (
            <div key={i} style={styles.diskRow}>
              <div>
                <div style={styles.label}>{disk.name}</div>
                <div style={{ fontSize: '14px', color: '#cdd6f4' }}>
                  {disk.used_gb} GB / {disk.total_gb} GB
                </div>
              </div>
              <div style={{ width: '120px' }}>
                <ProgressBar 
                  percent={disk.usage_percent} 
                  warning={disk.usage_percent > 80}
                />
              </div>
            </div>
          ))}
        </ResourceCard>
      )}

      {/* Timestamp */}
      <div style={{ ...styles.label, textAlign: 'center' }}>
        Last updated: {new Date(resources.timestamp * 1000).toLocaleTimeString()}
      </div>
    </div>
  );
}

export default SystemMonitor;