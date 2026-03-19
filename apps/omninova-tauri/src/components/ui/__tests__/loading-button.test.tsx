/**
 * Tests for LoadingButton Component
 *
 * [Source: Story 4.5 - 打字指示器与加载状态]
 */

import { describe, it, expect, vi } from 'vitest';
import { render, screen, fireEvent } from '@testing-library/react';
import { LoadingButton } from '@/components/ui/loading-button';

describe('LoadingButton', () => {
  describe('Rendering', () => {
    it('should render button with children', () => {
      render(<LoadingButton>Click me</LoadingButton>);

      expect(screen.getByRole('button', { name: 'Click me' })).toBeInTheDocument();
    });

    it('should render with default variant', () => {
      render(<LoadingButton>Default</LoadingButton>);

      const button = screen.getByRole('button');
      expect(button).toHaveClass('bg-primary');
    });

    it('should render with outline variant', () => {
      render(<LoadingButton variant="outline">Outline</LoadingButton>);

      const button = screen.getByRole('button');
      expect(button).toHaveClass('border-border');
    });

    it('should render with ghost variant', () => {
      render(<LoadingButton variant="ghost">Ghost</LoadingButton>);

      const button = screen.getByRole('button');
      expect(button).toHaveClass('hover:bg-muted');
    });
  });

  describe('Loading State', () => {
    it('should show spinner when loading', () => {
      const { container } = render(<LoadingButton loading>Loading</LoadingButton>);

      // Check for spinner SVG
      const spinner = container.querySelector('svg.animate-spin');
      expect(spinner).toBeInTheDocument();
    });

    it('should not show spinner when not loading', () => {
      const { container } = render(<LoadingButton>Not Loading</LoadingButton>);

      const spinner = container.querySelector('svg.animate-spin');
      expect(spinner).not.toBeInTheDocument();
    });

    it('should disable button when loading', () => {
      render(<LoadingButton loading>Loading</LoadingButton>);

      const button = screen.getByRole('button');
      expect(button).toBeDisabled();
    });

    it('should have aria-busy="true" when loading', () => {
      render(<LoadingButton loading>Loading</LoadingButton>);

      const button = screen.getByRole('button');
      expect(button).toHaveAttribute('aria-busy', 'true');
    });

    it('should not have aria-busy when not loading', () => {
      render(<LoadingButton>Not Loading</LoadingButton>);

      const button = screen.getByRole('button');
      expect(button).not.toHaveAttribute('aria-busy', 'true');
    });
  });

  describe('Loading Text', () => {
    it('should show loading text when provided', () => {
      render(
        <LoadingButton loading loadingText="Sending...">
          Send
        </LoadingButton>
      );

      expect(screen.getByText('Sending...')).toBeInTheDocument();
      expect(screen.queryByText('Send')).not.toBeInTheDocument();
    });

    it('should show children when loading but no loadingText', () => {
      render(<LoadingButton loading>Send</LoadingButton>);

      expect(screen.getByText('Send')).toBeInTheDocument();
    });
  });

  describe('Disabled State', () => {
    it('should be disabled when disabled prop is true', () => {
      render(<LoadingButton disabled>Disabled</LoadingButton>);

      const button = screen.getByRole('button');
      expect(button).toBeDisabled();
    });

    it('should be disabled when loading', () => {
      render(<LoadingButton loading>Loading</LoadingButton>);

      const button = screen.getByRole('button');
      expect(button).toBeDisabled();
    });

    it('should be disabled when both disabled and loading', () => {
      render(
        <LoadingButton disabled loading>
          Both
        </LoadingButton>
      );

      const button = screen.getByRole('button');
      expect(button).toBeDisabled();
    });
  });

  describe('Click Handling', () => {
    it('should call onClick when clicked', () => {
      const handleClick = vi.fn();
      render(<LoadingButton onClick={handleClick}>Click</LoadingButton>);

      fireEvent.click(screen.getByRole('button'));
      expect(handleClick).toHaveBeenCalledTimes(1);
    });

    it('should not call onClick when loading', () => {
      const handleClick = vi.fn();
      render(
        <LoadingButton loading onClick={handleClick}>
          Loading
        </LoadingButton>
      );

      // Button is disabled, so click doesn't fire
      expect(handleClick).not.toHaveBeenCalled();
    });

    it('should not call onClick when disabled', () => {
      const handleClick = vi.fn();
      render(
        <LoadingButton disabled onClick={handleClick}>
          Disabled
        </LoadingButton>
      );

      // Button is disabled, so click doesn't fire
      expect(handleClick).not.toHaveBeenCalled();
    });
  });

  describe('Size Variants', () => {
    it('should render default size', () => {
      render(<LoadingButton>Default Size</LoadingButton>);

      const button = screen.getByRole('button');
      expect(button).toHaveClass('h-8');
    });

    it('should render small size', () => {
      render(<LoadingButton size="sm">Small</LoadingButton>);

      const button = screen.getByRole('button');
      expect(button).toHaveClass('h-7');
    });

    it('should render large size', () => {
      render(<LoadingButton size="lg">Large</LoadingButton>);

      const button = screen.getByRole('button');
      expect(button).toHaveClass('h-9');
    });
  });

  describe('Custom Styling', () => {
    it('should accept custom className', () => {
      render(<LoadingButton className="custom-class">Custom</LoadingButton>);

      const button = screen.getByRole('button');
      expect(button).toHaveClass('custom-class');
    });
  });
});