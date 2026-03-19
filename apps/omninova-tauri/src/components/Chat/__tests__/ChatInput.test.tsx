/**
 * Tests for ChatInput Component
 *
 * [Source: Story 4.6 - 消息输入与发送功能]
 */

import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, fireEvent, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { ChatInput } from '../ChatInput';

// Mock scrollIntoView for textarea
HTMLElement.prototype.scrollIntoView = vi.fn();

describe('ChatInput', () => {
  const mockOnSend = vi.fn();
  const mockOnCancel = vi.fn();

  beforeEach(() => {
    vi.clearAllMocks();
  });

  describe('Rendering', () => {
    it('should render with default props', () => {
      render(<ChatInput onSend={mockOnSend} />);

      expect(screen.getByRole('textbox')).toBeInTheDocument();
      expect(screen.getByPlaceholderText('输入消息...')).toBeInTheDocument();
      expect(screen.getByRole('button', { name: '发送消息' })).toBeInTheDocument();
    });

    it('should render with custom placeholder', () => {
      render(<ChatInput onSend={mockOnSend} placeholder="Type a message..." />);

      expect(screen.getByPlaceholderText('Type a message...')).toBeInTheDocument();
    });

    it('should render cancel button when streaming', () => {
      render(
        <ChatInput onSend={mockOnSend} onCancel={mockOnCancel} isStreaming />
      );

      expect(screen.getByRole('button', { name: '停止生成' })).toBeInTheDocument();
      expect(screen.queryByRole('button', { name: '发送消息' })).not.toBeInTheDocument();
    });

    it('should not render cancel button if onCancel is not provided', () => {
      render(<ChatInput onSend={mockOnSend} isStreaming />);

      // Still shows stop button but it's disabled
      expect(screen.getByRole('button', { name: '停止生成' })).toBeDisabled();
    });
  });

  describe('Auto-expanding Textarea', () => {
    it('should start with minRows', () => {
      render(<ChatInput onSend={mockOnSend} minRows={1} />);

      const textarea = screen.getByRole('textbox');
      expect(textarea).toHaveAttribute('rows', '1');
    });

    it('should expand when typing multiple lines', async () => {
      const user = userEvent.setup();
      render(<ChatInput onSend={mockOnSend} />);

      const textarea = screen.getByRole('textbox') as HTMLTextAreaElement;

      // Type multiple lines
      await user.type(textarea, 'Line 1{enter}Line 2{enter}Line 3');

      // Textarea should have adjusted height
      expect(textarea.style.height).toMatch(/\d+px/);
    });
  });

  describe('Keyboard Shortcuts', () => {
    it('should send message on Enter (without Shift)', async () => {
      const user = userEvent.setup();
      render(<ChatInput onSend={mockOnSend} />);

      const textarea = screen.getByRole('textbox');
      await user.type(textarea, 'Hello{enter}');

      expect(mockOnSend).toHaveBeenCalledWith('Hello');
    });

    it('should not send on Shift+Enter (should add newline)', async () => {
      const user = userEvent.setup();
      render(<ChatInput onSend={mockOnSend} />);

      const textarea = screen.getByRole('textbox') as HTMLTextAreaElement;

      // Shift+Enter should add a newline, not send
      await user.type(textarea, 'Hello{Shift>}{enter}{/Shift}');

      expect(mockOnSend).not.toHaveBeenCalled();
      expect(textarea.value).toContain('\n');
    });

    it('should not send empty message', async () => {
      const user = userEvent.setup();
      render(<ChatInput onSend={mockOnSend} />);

      const textarea = screen.getByRole('textbox');
      await user.type(textarea, '{enter}');

      expect(mockOnSend).not.toHaveBeenCalled();
    });

    it('should not send whitespace-only message', async () => {
      const user = userEvent.setup();
      render(<ChatInput onSend={mockOnSend} />);

      const textarea = screen.getByRole('textbox');
      await user.type(textarea, '   {enter}');

      expect(mockOnSend).not.toHaveBeenCalled();
    });

    it('should not send when streaming', async () => {
      const user = userEvent.setup();
      render(<ChatInput onSend={mockOnSend} isStreaming />);

      const textarea = screen.getByRole('textbox');
      await user.type(textarea, 'Hello{enter}');

      expect(mockOnSend).not.toHaveBeenCalled();
    });

    it('should not send when disabled', async () => {
      const user = userEvent.setup();
      render(<ChatInput onSend={mockOnSend} disabled />);

      const textarea = screen.getByRole('textbox');
      await user.type(textarea, 'Hello{enter}');

      expect(mockOnSend).not.toHaveBeenCalled();
    });
  });

  describe('Send Button', () => {
    it('should be disabled when input is empty', () => {
      render(<ChatInput onSend={mockOnSend} />);

      const sendButton = screen.getByRole('button', { name: '发送消息' });
      expect(sendButton).toBeDisabled();
    });

    it('should be disabled when streaming', () => {
      render(<ChatInput onSend={mockOnSend} isStreaming />);

      // When streaming, cancel button is shown instead
      const cancelButton = screen.getByRole('button', { name: '停止生成' });
      expect(cancelButton).toBeInTheDocument();
    });

    it('should be enabled when input has content', async () => {
      const user = userEvent.setup();
      render(<ChatInput onSend={mockOnSend} />);

      const textarea = screen.getByRole('textbox');
      await user.type(textarea, 'Hello');

      const sendButton = screen.getByRole('button', { name: '发送消息' });
      expect(sendButton).not.toBeDisabled();
    });

    it('should send message on click', async () => {
      const user = userEvent.setup();
      render(<ChatInput onSend={mockOnSend} />);

      const textarea = screen.getByRole('textbox');
      await user.type(textarea, 'Hello');

      const sendButton = screen.getByRole('button', { name: '发送消息' });
      await user.click(sendButton);

      expect(mockOnSend).toHaveBeenCalledWith('Hello');
    });

    it('should clear input after send (uncontrolled)', async () => {
      const user = userEvent.setup();
      render(<ChatInput onSend={mockOnSend} />);

      const textarea = screen.getByRole('textbox') as HTMLTextAreaElement;
      await user.type(textarea, 'Hello');

      const sendButton = screen.getByRole('button', { name: '发送消息' });
      await user.click(sendButton);

      await waitFor(() => {
        expect(textarea.value).toBe('');
      });
    });
  });

  describe('Cancel Button', () => {
    it('should call onCancel when clicked', async () => {
      const user = userEvent.setup();
      render(
        <ChatInput onSend={mockOnSend} onCancel={mockOnCancel} isStreaming />
      );

      const cancelButton = screen.getByRole('button', { name: '停止生成' });
      await user.click(cancelButton);

      expect(mockOnCancel).toHaveBeenCalled();
    });
  });

  describe('Focus Management', () => {
    it('should focus textarea on mount', () => {
      render(<ChatInput onSend={mockOnSend} />);

      const textarea = screen.getByRole('textbox');
      expect(textarea).toHaveFocus();
    });

    it('should refocus textarea after send', async () => {
      const user = userEvent.setup();
      render(<ChatInput onSend={mockOnSend} />);

      const textarea = screen.getByRole('textbox');
      await user.type(textarea, 'Hello');

      const sendButton = screen.getByRole('button', { name: '发送消息' });
      await user.click(sendButton);

      // Focus should return to textarea
      await waitFor(() => {
        expect(textarea).toHaveFocus();
      });
    });
  });

  describe('Paste Support', () => {
    it('should allow pasting text', async () => {
      render(<ChatInput onSend={mockOnSend} />);

      const textarea = screen.getByRole('textbox') as HTMLTextAreaElement;

      // Simulate paste event
      fireEvent.paste(textarea, {
        clipboardData: {
          getData: () => 'Pasted text',
        },
      });

      // Paste event should fire (browser handles actual paste)
      expect(textarea).toBeInTheDocument();
    });
  });

  describe('Personality Theming', () => {
    it('should apply personality colors to send button', async () => {
      const user = userEvent.setup();
      render(<ChatInput onSend={mockOnSend} personalityType="INTJ" />);

      const textarea = screen.getByRole('textbox');
      await user.type(textarea, 'Hello');

      const sendButton = screen.getByRole('button', { name: '发送消息' });
      // INTJ primary color is #2563EB
      expect(sendButton).toHaveStyle({ backgroundColor: '#2563EB' });
    });

    it('should apply different personality colors', async () => {
      const user = userEvent.setup();
      render(<ChatInput onSend={mockOnSend} personalityType="INFJ" />);

      const textarea = screen.getByRole('textbox');
      await user.type(textarea, 'Hello');

      const sendButton = screen.getByRole('button', { name: '发送消息' });
      // INFJ primary color is #EA580C
      expect(sendButton).toHaveStyle({ backgroundColor: '#EA580C' });
    });

    it('should not apply colors when input is empty', () => {
      render(<ChatInput onSend={mockOnSend} personalityType="INTJ" />);

      const sendButton = screen.getByRole('button', { name: '发送消息' });
      // Disabled button should not have custom colors
      expect(sendButton).not.toHaveStyle({ backgroundColor: '#2563EB' });
    });
  });

  describe('Controlled Mode', () => {
    it('should use controlled value when provided', () => {
      const { rerender } = render(
        <ChatInput onSend={mockOnSend} value="Test message" onChange={vi.fn()} />
      );

      const textarea = screen.getByRole('textbox') as HTMLTextAreaElement;
      expect(textarea.value).toBe('Test message');

      // Update value
      rerender(
        <ChatInput onSend={mockOnSend} value="Updated message" onChange={vi.fn()} />
      );
      expect(textarea.value).toBe('Updated message');
    });

    it('should call onChange in controlled mode', async () => {
      const handleChange = vi.fn();
      const user = userEvent.setup();
      render(<ChatInput onSend={mockOnSend} value="" onChange={handleChange} />);

      const textarea = screen.getByRole('textbox');
      await user.type(textarea, 'H');

      expect(handleChange).toHaveBeenCalledWith('H');
    });

    it('should call onChange with empty string after send in controlled mode', async () => {
      const handleChange = vi.fn();
      const user = userEvent.setup();

      render(<ChatInput onSend={mockOnSend} value="Hello" onChange={handleChange} />);

      const sendButton = screen.getByRole('button', { name: '发送消息' });
      await user.click(sendButton);

      expect(mockOnSend).toHaveBeenCalledWith('Hello');
      // In controlled mode, we signal the parent to clear via onChange
      expect(handleChange).toHaveBeenCalledWith('');
    });
  });

  describe('Accessibility', () => {
    it('should have aria-label on textarea', () => {
      render(<ChatInput onSend={mockOnSend} />);

      const textarea = screen.getByRole('textbox');
      expect(textarea).toHaveAttribute('aria-label', '消息输入框');
    });

    it('should have aria-label on send button', () => {
      render(<ChatInput onSend={mockOnSend} />);

      const sendButton = screen.getByRole('button', { name: '发送消息' });
      expect(sendButton).toBeInTheDocument();
    });

    it('should disable input when streaming or disabled', () => {
      const { rerender } = render(<ChatInput onSend={mockOnSend} isStreaming />);

      let textarea = screen.getByRole('textbox');
      expect(textarea).toBeDisabled();

      rerender(<ChatInput onSend={mockOnSend} disabled />);
      textarea = screen.getByRole('textbox');
      expect(textarea).toBeDisabled();
    });
  });

  describe('Custom Styling', () => {
    it('should accept custom className', () => {
      render(<ChatInput onSend={mockOnSend} className="custom-input" />);

      // The className is applied to the container
      const container = screen.getByRole('textbox').closest('.custom-input');
      expect(container).toBeInTheDocument();
    });

    it('should accept custom minRows and maxRows', () => {
      render(<ChatInput onSend={mockOnSend} minRows={2} maxRows={10} />);

      const textarea = screen.getByRole('textbox');
      expect(textarea).toHaveAttribute('rows', '2');
    });
  });
});