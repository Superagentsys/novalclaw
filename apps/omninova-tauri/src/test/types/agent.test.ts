/**
 * Agent 类型定义测试
 *
 * 测试 AgentModel, NewAgent, AgentUpdate 等类型
 */

import { describe, it, expect } from 'vitest';
import type { AgentModel, NewAgent, AgentUpdate } from '@/types/agent';
import type { MBTIType } from '@/lib/personality-colors';

describe('Agent Types', () => {
  describe('AgentModel', () => {
    it('should have all required fields', () => {
      const agent: AgentModel = {
        id: 1,
        agent_uuid: 'test-uuid',
        name: 'Test Agent',
        status: 'active',
        created_at: Date.now(),
        updated_at: Date.now(),
      };

      expect(agent.id).toBe(1);
      expect(agent.name).toBe('Test Agent');
    });

    it('should allow optional fields', () => {
      const agent: AgentModel = {
        id: 1,
        agent_uuid: 'test-uuid',
        name: 'Test Agent',
        description: 'A test agent',
        domain: 'Testing',
        mbti_type: 'INTJ' as MBTIType,
        status: 'active',
        system_prompt: 'You are a test agent',
        created_at: Date.now(),
        updated_at: Date.now(),
      };

      expect(agent.description).toBe('A test agent');
      expect(agent.domain).toBe('Testing');
      expect(agent.mbti_type).toBe('INTJ');
    });
  });

  describe('NewAgent', () => {
    it('should require name and allow optional fields', () => {
      const newAgent: NewAgent = {
        name: 'New Agent',
        description: 'Description',
        domain: 'Domain',
        mbti_type: 'ENFP' as MBTIType,
      };

      expect(newAgent.name).toBe('New Agent');
    });
  });

  describe('AgentUpdate', () => {
    it('should allow partial updates with all optional fields', () => {
      // Test: AgentUpdate should have all fields as optional
      const update: AgentUpdate = {
        name: 'Updated Name',
        description: 'Updated Description',
        domain: 'Updated Domain',
        mbti_type: 'INFP' as MBTIType,
        system_prompt: 'Updated prompt',
      };

      expect(update.name).toBe('Updated Name');
      expect(update.description).toBe('Updated Description');
      expect(update.domain).toBe('Updated Domain');
      expect(update.mbti_type).toBe('INFP');
      expect(update.system_prompt).toBe('Updated prompt');
    });

    it('should allow empty object for no updates', () => {
      // Test: AgentUpdate should allow empty object
      const update: AgentUpdate = {};

      expect(Object.keys(update)).toHaveLength(0);
    });

    it('should allow single field updates', () => {
      // Test name only
      const nameOnly: AgentUpdate = { name: 'New Name' };
      expect(nameOnly.name).toBe('New Name');

      // Test description only
      const descOnly: AgentUpdate = { description: 'New Desc' };
      expect(descOnly.description).toBe('New Desc');

      // Test mbti_type only
      const mbtiOnly: AgentUpdate = { mbti_type: 'ENTJ' as MBTIType };
      expect(mbtiOnly.mbti_type).toBe('ENTJ');
    });
  });
});