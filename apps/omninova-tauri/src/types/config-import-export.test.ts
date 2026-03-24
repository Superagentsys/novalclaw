/**
 * Config Import/Export Types Tests
 *
 * Tests for configuration import/export utility functions.
 * [Source: Story 7.8 - 配置导入导出功能]
 */

import { describe, it, expect } from 'vitest';
import {
  checkVersionCompatibility,
  detectFormat,
  isSensitiveField,
  filterSensitiveData,
  CURRENT_EXPORT_VERSION,
} from './config-import-export';

describe('config-import-export types', () => {
  describe('checkVersionCompatibility', () => {
    it('should return true for same major version', () => {
      expect(checkVersionCompatibility('1.0.0')).toBe(true);
      expect(checkVersionCompatibility('1.2.3')).toBe(true);
      expect(checkVersionCompatibility('1.9.9')).toBe(true);
    });

    it('should return false for different major version', () => {
      expect(checkVersionCompatibility('2.0.0')).toBe(false);
      expect(checkVersionCompatibility('0.1.0')).toBe(false);
      expect(checkVersionCompatibility('3.0.0')).toBe(false);
    });

    it('should handle invalid version strings', () => {
      expect(checkVersionCompatibility('')).toBe(false);
      expect(checkVersionCompatibility('invalid')).toBe(false);
    });
  });

  describe('detectFormat', () => {
    it('should detect JSON format', () => {
      expect(detectFormat('{"key": "value"}')).toBe('json');
      expect(detectFormat('[1, 2, 3]')).toBe('json');
      expect(detectFormat('  { "test": true }  ')).toBe('json');
    });

    it('should return null for ambiguous content', () => {
      expect(detectFormat('')).toBeNull();
      expect(detectFormat('plain text')).toBeNull();
      expect(detectFormat('no colons here')).toBeNull();
    });
  });

  describe('isSensitiveField', () => {
    it('should identify sensitive fields', () => {
      expect(isSensitiveField('apiKey')).toBe(true);
      expect(isSensitiveField('api_key')).toBe(true);
      expect(isSensitiveField('password')).toBe(true);
      expect(isSensitiveField('token')).toBe(true);
      expect(isSensitiveField('accessToken')).toBe(true);
      expect(isSensitiveField('secret')).toBe(true);
    });

    it('should be case-insensitive', () => {
      expect(isSensitiveField('APIKEY')).toBe(true);
      expect(isSensitiveField('API_KEY')).toBe(true);
      expect(isSensitiveField('Password')).toBe(true);
      expect(isSensitiveField('TOKEN')).toBe(true);
    });

    it('should identify non-sensitive fields', () => {
      expect(isSensitiveField('name')).toBe(false);
      expect(isSensitiveField('description')).toBe(false);
      expect(isSensitiveField('mbtiType')).toBe(false);
      expect(isSensitiveField('domain')).toBe(false);
    });
  });

  describe('filterSensitiveData', () => {
    it('should remove sensitive fields from object', () => {
      const input = {
        name: 'test',
        apiKey: 'secret-key',
        description: 'test desc',
      };

      const result = filterSensitiveData(input);

      expect(result.name).toBe('test');
      expect(result.description).toBe('test desc');
      expect(result).not.toHaveProperty('apiKey');
    });

    it('should recursively filter nested objects', () => {
      const input = {
        name: 'test',
        config: {
          provider: 'openai',
          apiKey: 'sk-test',
          nested: {
            secret: 'hidden',
            value: 'visible',
          },
        },
      };

      const result = filterSensitiveData(input);

      expect(result.name).toBe('test');
      expect(result.config.provider).toBe('openai');
      expect(result.config).not.toHaveProperty('apiKey');
      expect(result.config.nested).not.toHaveProperty('secret');
      expect(result.config.nested.value).toBe('visible');
    });

    it('should filter arrays of objects', () => {
      const input = {
        items: [
          { name: 'item1', token: 't1' },
          { name: 'item2', token: 't2' },
        ],
      };

      const result = filterSensitiveData(input);

      expect(result.items[0].name).toBe('item1');
      expect(result.items[0]).not.toHaveProperty('token');
      expect(result.items[1].name).toBe('item2');
      expect(result.items[1]).not.toHaveProperty('token');
    });

    it('should preserve null and undefined values', () => {
      const input = {
        name: 'test',
        description: null,
        domain: undefined,
      };

      const result = filterSensitiveData(input);

      expect(result.name).toBe('test');
      expect(result.description).toBeNull();
      expect(result.domain).toBeUndefined();
    });

    it('should prevent infinite recursion', () => {
      const input: Record<string, unknown> = { name: 'test' };
      input.self = input; // Circular reference

      // Should not throw
      const result = filterSensitiveData(input);
      expect(result.name).toBe('test');
    });
  });

  describe('CURRENT_EXPORT_VERSION', () => {
    it('should be a valid semver string', () => {
      expect(CURRENT_EXPORT_VERSION).toMatch(/^\d+\.\d+\.\d+$/);
    });

    it('should start with 1', () => {
      expect(CURRENT_EXPORT_VERSION.startsWith('1.')).toBe(true);
    });
  });
});