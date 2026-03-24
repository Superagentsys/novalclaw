/**
 * Configuration Types Tests
 *
 * Tests for configuration type utilities.
 * [Source: Story 7.7 - ConfigurationPanel 组件]
 */

import { describe, it, expect } from 'vitest';
import {
  createDefaultAgentConfiguration,
  getConfigChanges,
  validateConfiguration,
  type AgentConfiguration,
} from './configuration';

describe('configuration types', () => {
  const agentId = 'test-agent-id';

  describe('createDefaultAgentConfiguration', () => {
    it('should create default configuration with correct agentId', () => {
      const config = createDefaultAgentConfiguration(agentId);

      expect(config.agentId).toBe(agentId);
      expect(config.styleConfig).toBeDefined();
      expect(config.contextConfig).toBeDefined();
      expect(config.triggerConfig).toBeDefined();
      expect(config.privacyConfig).toBeDefined();
      expect(config.skillConfig).toBeDefined();
    });

    it('should have correct default style config values', () => {
      const config = createDefaultAgentConfiguration(agentId);

      expect(config.styleConfig.responseStyle).toBe('detailed');
      expect(config.styleConfig.verbosity).toBe(0.5);
      expect(config.styleConfig.maxResponseLength).toBe(0);
    });

    it('should have correct default context config values', () => {
      const config = createDefaultAgentConfiguration(agentId);

      expect(config.contextConfig.maxTokens).toBe(4096);
      expect(config.contextConfig.responseReserve).toBe(1024);
    });

    it('should have correct default trigger config values', () => {
      const config = createDefaultAgentConfiguration(agentId);

      expect(config.triggerConfig.keywords).toEqual([]);
      expect(config.triggerConfig.defaultMatchType).toBe('exact');
    });

    it('should have correct default privacy config values', () => {
      const config = createDefaultAgentConfiguration(agentId);

      expect(config.privacyConfig.dataRetention.episodicMemoryDays).toBe(90);
      expect(config.privacyConfig.dataRetention.workingMemoryHours).toBe(24);
      expect(config.privacyConfig.sensitiveFilter.enabled).toBe(false);
    });
  });

  describe('getConfigChanges', () => {
    it('should return empty array when configs are equal', () => {
      const config = createDefaultAgentConfiguration(agentId);
      const changes = getConfigChanges(config, config);

      expect(changes).toEqual([]);
    });

    it('should detect style config changes', () => {
      const original = createDefaultAgentConfiguration(agentId);
      const current: AgentConfiguration = {
        ...original,
        styleConfig: {
          ...original.styleConfig,
          responseStyle: 'concise',
        },
      };

      const changes = getConfigChanges(original, current);

      expect(changes.length).toBeGreaterThan(0);
      expect(changes.some(c => c.path.includes('responseStyle'))).toBe(true);
    });

    it('should detect context config changes', () => {
      const original = createDefaultAgentConfiguration(agentId);
      const current: AgentConfiguration = {
        ...original,
        contextConfig: {
          ...original.contextConfig,
          maxTokens: 8192,
        },
      };

      const changes = getConfigChanges(original, current);

      expect(changes.some(c => c.path.includes('maxTokens'))).toBe(true);
    });

    it('should detect trigger config changes', () => {
      const original = createDefaultAgentConfiguration(agentId);
      const current: AgentConfiguration = {
        ...original,
        triggerConfig: {
          ...original.triggerConfig,
          keywords: ['hello', 'world'],
        },
      };

      const changes = getConfigChanges(original, current);

      expect(changes.some(c => c.path.includes('keywords'))).toBe(true);
    });

    it('should detect privacy config changes', () => {
      const original = createDefaultAgentConfiguration(agentId);
      const current: AgentConfiguration = {
        ...original,
        privacyConfig: {
          ...original.privacyConfig,
          dataRetention: {
            ...original.privacyConfig.dataRetention,
            episodicMemoryDays: 7,
          },
        },
      };

      const changes = getConfigChanges(original, current);

      expect(changes.some(c => c.path.includes('episodicMemoryDays'))).toBe(true);
    });

    it('should detect multiple changes', () => {
      const original = createDefaultAgentConfiguration(agentId);
      const current: AgentConfiguration = {
        ...original,
        styleConfig: { ...original.styleConfig, responseStyle: 'concise' },
        contextConfig: { ...original.contextConfig, maxTokens: 8192 },
      };

      const changes = getConfigChanges(original, current);

      expect(changes.length).toBeGreaterThanOrEqual(2);
    });

    it('should include old and new values', () => {
      const original = createDefaultAgentConfiguration(agentId);
      const current: AgentConfiguration = {
        ...original,
        styleConfig: {
          ...original.styleConfig,
          responseStyle: 'concise',
        },
      };

      const changes = getConfigChanges(original, current);

      const styleChange = changes.find(c => c.path.includes('responseStyle'));
      expect(styleChange).toBeDefined();
      expect(styleChange!.oldValue).toBe('detailed');
      expect(styleChange!.newValue).toBe('concise');
    });
  });

  describe('validateConfiguration', () => {
    it('should return valid for default configuration', () => {
      const config = createDefaultAgentConfiguration(agentId);
      const result = validateConfiguration(config);

      expect(result.isValid).toBe(true);
      expect(result.errors).toEqual([]);
    });

    it('should validate verbosity range', () => {
      const config = createDefaultAgentConfiguration(agentId);
      config.styleConfig.verbosity = -1;

      const result = validateConfiguration(config);

      expect(result.isValid).toBe(false);
      expect(result.errors.some(e => e.path.includes('verbosity'))).toBe(true);
    });

    it('should validate maxTokens is non-negative', () => {
      const config = createDefaultAgentConfiguration(agentId);
      config.contextConfig.maxTokens = -100;

      const result = validateConfiguration(config);

      expect(result.isValid).toBe(false);
      expect(result.errors.some(e => e.path.includes('maxTokens'))).toBe(true);
    });

    it('should validate responseReserve is non-negative', () => {
      const config = createDefaultAgentConfiguration(agentId);
      config.contextConfig.responseReserve = -50;

      const result = validateConfiguration(config);

      expect(result.isValid).toBe(false);
      expect(result.errors.some(e => e.path.includes('responseReserve'))).toBe(true);
    });

    it('should validate maxResponseLength is non-negative', () => {
      const config = createDefaultAgentConfiguration(agentId);
      config.styleConfig.maxResponseLength = -10;

      const result = validateConfiguration(config);

      expect(result.isValid).toBe(false);
      expect(result.errors.some(e => e.path.includes('maxResponseLength'))).toBe(true);
    });

    it('should validate episodicMemoryDays is non-negative', () => {
      const config = createDefaultAgentConfiguration(agentId);
      config.privacyConfig.dataRetention.episodicMemoryDays = -1;

      const result = validateConfiguration(config);

      expect(result.isValid).toBe(false);
      expect(result.errors.some(e => e.path.includes('episodicMemoryDays'))).toBe(true);
    });

    it('should validate workingMemoryHours is non-negative', () => {
      const config = createDefaultAgentConfiguration(agentId);
      config.privacyConfig.dataRetention.workingMemoryHours = -1;

      const result = validateConfiguration(config);

      expect(result.isValid).toBe(false);
      expect(result.errors.some(e => e.path.includes('workingMemoryHours'))).toBe(true);
    });

    it('should return multiple errors for multiple invalid values', () => {
      const config = createDefaultAgentConfiguration(agentId);
      config.styleConfig.verbosity = -1;
      config.contextConfig.maxTokens = -100;

      const result = validateConfiguration(config);

      expect(result.isValid).toBe(false);
      expect(result.errors.length).toBeGreaterThanOrEqual(2);
    });
  });
});