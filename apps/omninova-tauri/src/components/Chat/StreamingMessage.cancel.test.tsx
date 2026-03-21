/**
 * Tests for StreamingMessage cancellation functionality
 *
 * Story 4.9: 响应中断功能
 *
 * Tests for:
 * - isCancelled prop display
 * - "[已中断]" marker visibility
 * - Cancel button visibility when cancelled
 */

import { describe, it, expect, vi } from 'vitest';
import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { StreamingMessage } from './StreamingMessage';

describe('StreamingMessage - Cancellation (Story 4.9)', () => {
  describe('isCancelled prop', () => {
    it('should not show cancelled marker when isCancelled is false', () => {
      render(
        <StreamingMessage
          content="This is a response"
          isStreaming={false}
          isCancelled={false}
        />
      );

      expect(screen.queryByText('已中断')).not.toBeInTheDocument();
    });

    it('should show "[已中断]" marker when isCancelled is true', () => {
      render(
        <StreamingMessage
          content="This is a partial response"
          isStreaming={false}
          isCancelled={true}
        />
      );

      expect(screen.getByText('已中断')).toBeInTheDocument();
    });

    it('should show marker with warning icon when cancelled', () => {
      render(
        <StreamingMessage
          content="Partial content"
          isStreaming={false}
          isCancelled={true}
        />
      );

      // The marker should have the aria-label
      const marker = screen.getByLabelText('响应已被中断');
      expect(marker).toBeInTheDocument();
    });
  });

  describe('cancel button visibility', () => {
    it('should show cancel button when streaming and not cancelled', () => {
      const onCancel = vi.fn();

      render(
        <StreamingMessage
          content="Streaming..."
          isStreaming={true}
          isCancelled={false}
          onCancel={onCancel}
        />
      );

      expect(screen.getByRole('button', { name: '停止生成' })).toBeInTheDocument();
    });

    it('should not show cancel button when cancelled', () => {
      const onCancel = vi.fn();

      render(
        <StreamingMessage
          content="Partial content"
          isStreaming={false}
          isCancelled={true}
          onCancel={onCancel}
        />
      );

      expect(screen.queryByRole('button', { name: '停止生成' })).not.toBeInTheDocument();
    });

    it('should not show cancel button when not streaming', () => {
      const onCancel = vi.fn();

      render(
        <StreamingMessage
          content="Complete response"
          isStreaming={false}
          isCancelled={false}
          onCancel={onCancel}
        />
      );

      expect(screen.queryByRole('button', { name: '停止生成' })).not.toBeInTheDocument();
    });
  });

  describe('cancelled state interaction', () => {
    it('should render markdown content when cancelled', () => {
      render(
        <StreamingMessage
          content="Here is some **bold** text and `code`"
          isStreaming={false}
          isCancelled={true}
        />
      );

      // Check that the content is rendered
      expect(screen.getByText(/Here is some/)).toBeInTheDocument();
      // Check that bold is rendered
      expect(screen.getByText('bold').tagName).toBe('STRONG');
      // Check that the marker is also shown
      expect(screen.getByText('已中断')).toBeInTheDocument();
    });

    it('should work with agentName when cancelled', () => {
      render(
        <StreamingMessage
          content="Partial response"
          isStreaming={false}
          isCancelled={true}
          agentName="Test Agent"
        />
      );

      expect(screen.getByText('Test Agent')).toBeInTheDocument();
      expect(screen.getByText('已中断')).toBeInTheDocument();
    });
  });

  describe('accessibility', () => {
    it('should have aria-live="polite" for cancelled content', () => {
      render(
        <StreamingMessage
          content="Interrupted response"
          isStreaming={false}
          isCancelled={true}
        />
      );

      const article = screen.getByRole('article');
      expect(article).toHaveAttribute('aria-live', 'polite');
    });

    it('should have aria-label on cancelled marker', () => {
      render(
        <StreamingMessage
          content="Partial"
          isStreaming={false}
          isCancelled={true}
        />
      );

      const marker = screen.getByLabelText('响应已被中断');
      expect(marker).toBeInTheDocument();
    });
  });
});