/**
 * Session Grouping Tests
 *
 * Unit tests for session time-based grouping.
 *
 * [Source: Story 10.2 - 历史对话导航]
 */

import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest';
import {
  getSessionGroup,
  groupSessionsByTime,
  SessionTimeGroup,
} from './sessionGrouping';

describe('getSessionGroup', () => {
  // Mock current date to make tests deterministic
  const mockNow = new Date('2026-03-27T12:00:00');

  beforeEach(() => {
    vi.useFakeTimers();
    vi.setSystemTime(mockNow);
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  it('should return "today" for today', () => {
    const today = new Date('2026-03-27T08:30:00');
    expect(getSessionGroup(today)).toBe('today');
  });

  it('should return "today" for just now', () => {
    const now = new Date('2026-03-27T11:59:59');
    expect(getSessionGroup(now)).toBe('today');
  });

  it('should return "yesterday" for yesterday', () => {
    const yesterday = new Date('2026-03-26T15:00:00');
    expect(getSessionGroup(yesterday)).toBe('yesterday');
  });

  it('should return "yesterday" for yesterday midnight', () => {
    const yesterday = new Date('2026-03-26T00:00:00');
    expect(getSessionGroup(yesterday)).toBe('yesterday');
  });

  it('should return "thisWeek" for 3 days ago', () => {
    const threeDaysAgo = new Date('2026-03-24T10:00:00');
    expect(getSessionGroup(threeDaysAgo)).toBe('thisWeek');
  });

  it('should return "thisWeek" for 6 days ago', () => {
    const sixDaysAgo = new Date('2026-03-21T10:00:00');
    expect(getSessionGroup(sixDaysAgo)).toBe('thisWeek');
  });

  it('should return "older" for 8 days ago', () => {
    const eightDaysAgo = new Date('2026-03-19T10:00:00');
    expect(getSessionGroup(eightDaysAgo)).toBe('older');
  });

  it('should return "older" for a month ago', () => {
    const monthAgo = new Date('2026-02-27T10:00:00');
    expect(getSessionGroup(monthAgo)).toBe('older');
  });
});

describe('groupSessionsByTime', () => {
  const mockNow = new Date('2026-03-27T12:00:00');

  beforeEach(() => {
    vi.useFakeTimers();
    vi.setSystemTime(mockNow);
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  it('should group sessions correctly', () => {
    const sessions = [
      { id: 1, createdAt: '2026-03-27T08:00:00' }, // today
      { id: 2, createdAt: '2026-03-26T10:00:00' }, // yesterday
      { id: 3, createdAt: '2026-03-24T10:00:00' }, // thisWeek
      { id: 4, createdAt: '2026-03-15T10:00:00' }, // older
    ];

    const groups = groupSessionsByTime(sessions);

    expect(groups.today).toHaveLength(1);
    expect(groups.today[0].id).toBe(1);

    expect(groups.yesterday).toHaveLength(1);
    expect(groups.yesterday[0].id).toBe(2);

    expect(groups.thisWeek).toHaveLength(1);
    expect(groups.thisWeek[0].id).toBe(3);

    expect(groups.older).toHaveLength(1);
    expect(groups.older[0].id).toBe(4);
  });

  it('should handle empty sessions', () => {
    const groups = groupSessionsByTime([]);

    expect(groups.today).toHaveLength(0);
    expect(groups.yesterday).toHaveLength(0);
    expect(groups.thisWeek).toHaveLength(0);
    expect(groups.older).toHaveLength(0);
  });

  it('should handle Date objects', () => {
    const sessions = [
      { id: 1, createdAt: new Date('2026-03-27T08:00:00') },
    ];

    const groups = groupSessionsByTime(sessions);

    expect(groups.today).toHaveLength(1);
  });
});