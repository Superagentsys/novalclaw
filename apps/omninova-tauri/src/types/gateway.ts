/**
 * Gateway Types
 *
 * Type definitions for HTTP Gateway functionality
 *
 * [Source: Story 8.1 - HTTP Gateway 服务实现]
 */

/**
 * Gateway status payload returned by Tauri commands
 */
export interface GatewayStatusPayload {
  running: boolean;
  url: string;
  lastError?: string;
}

/**
 * Gateway health check result
 */
export interface GatewayHealth {
  status: 'healthy' | 'degraded' | 'unhealthy';
  uptime_seconds: number;
  requests_total: number;
  version: string;
  timestamp: string;
}

/**
 * Gateway configuration
 */
export interface GatewayConfig {
  enabled: boolean;
  host: string;
  port: number;
  cors: CorsConfig;
  tls: TlsConfig;
}

/**
 * CORS configuration
 */
export interface CorsConfig {
  enabled: boolean;
  allowed_origins: string[];
  allowed_methods: string[];
  allowed_headers: string[];
  allow_credentials: boolean;
  max_age: number;
}

/**
 * TLS/HTTPS configuration
 */
export interface TlsConfig {
  enabled: boolean;
  cert_path?: string;
  key_path?: string;
}

/**
 * Default gateway configuration
 * Note: Default port matches backend default (42617)
 */
export const defaultGatewayConfig: GatewayConfig = {
  enabled: true,
  host: '127.0.0.1',
  port: 42617,
  cors: {
    enabled: true,
    allowed_origins: ['*'],
    allowed_methods: ['GET', 'POST', 'PUT', 'DELETE', 'OPTIONS'],
    allowed_headers: ['Content-Type', 'Authorization'],
    allow_credentials: false,
    max_age: 3600,
  },
  tls: {
    enabled: false,
  },
};