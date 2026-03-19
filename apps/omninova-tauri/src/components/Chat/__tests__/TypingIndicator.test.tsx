/**
 * Tests for TypingIndicator Component
 *
 * [Source: Story 4.5 - 打字指示器与加载状态]
 */

import { describe, it, expect, vi } from 'vitest';
import { render, screen } from '@testing-library/react';
import { TypingIndicator } from '../TypingIndicator';

// Mock matchMedia for reduced motion detection
const mockMatchMedia = vi.fn();
Object.defineProperty(window, 'matchMedia', {
  writable: true,
  value: mockMatchMedia.mockImplementation((query: string) => ({
    matches: false,
    media: query,
    onchange: null,
    addListener: vi.fn(),
    removeListener: vi.fn(),
    addEventListener: vi.fn(),
    removeEventListener: vi.fn(),
    dispatchEvent: vi.fn(),
  })),
});

describe('TypingIndicator', () => {
  describe('Rendering', () => {
    it('should render with default props', () => {
      render(<TypingIndicator />);

      // Should have role="status" for accessibility
      expect(screen.getByRole('status')).toBeInTheDocument();
    });

    it('should render with label when showLabel is true', () => {
      render(<TypingIndicator showLabel label="正在思考..." />);

      expect(screen.getByText('正在思考...')).toBeInTheDocument();
    });

    it('should render with custom label', () => {
      render(<TypingIndicator showLabel label="Loading..." />);

      expect(screen.getByText('Loading...')).toBeInTheDocument();
    });

    it('should have aria-label for accessibility', () => {
      render(<TypingIndicator />);

      expect(screen.getByLabelText('正在输入')).toBeInTheDocument();
    });

    it('should have aria-live="polite" for screen readers', () => {
      render(<TypingIndicator />);

      const indicator = screen.getByRole('status');
      expect(indicator).toHaveAttribute('aria-live', 'polite');
    });
  });

  describe('Personality Theming', () => {
    it('should render with INTJ personality (analytical tone)', () => {
      const { container } = render(<TypingIndicator personalityType="INTJ" />);

      // Should render dots animation for analytical tone
      const dots = container.querySelectorAll('[class*="rounded-full"]');
      expect(dots.length).toBe(3);
    });

    it('should render with INFP personality (creative tone)', () => {
      const { container } = render(<TypingIndicator personalityType="INFP" />);

      // Should render wave animation for creative tone
      const container_div = container.firstChild;
      expect(container_div).toBeInTheDocument();
    });

    it('should render with ISTJ personality (structured tone)', () => {
      const { container } = render(<TypingIndicator personalityType="ISTJ" />);

      // Should render pulse animation for structured tone
      const container_div = container.firstChild;
      expect(container_div).toBeInTheDocument();
    });

    it('should render with ESTP personality (energetic tone)', () => {
      const { container } = render(<TypingIndicator personalityType="ESTP" />);

      // Should render dots animation for energetic tone
      const dots = container.querySelectorAll('[class*="rounded-full"]');
      expect(dots.length).toBe(3);
    });
  });

  describe('Animation Styles', () => {
    it('should render dots animation style', () => {
      const { container } = render(<TypingIndicator animationStyle="dots" />);

      // Check for animate-bounce class
      const dots = container.querySelectorAll('.animate-bounce');
      expect(dots.length).toBe(3);
    });

    it('should render pulse animation style', () => {
      const { container } = render(<TypingIndicator animationStyle="pulse" />);

      // Check for animate-pulse class
      const dots = container.querySelectorAll('.animate-pulse');
      expect(dots.length).toBe(3);
    });

    it('should render wave animation style', () => {
      const { container } = render(<TypingIndicator animationStyle="wave" />);

      // Wave animation is applied via style
      const container_div = container.firstChild;
      expect(container_div).toBeInTheDocument();
    });
  });

  describe('Size Variants', () => {
    it('should render small size', () => {
      const { container } = render(<TypingIndicator size="sm" />);

      const smallDots = container.querySelectorAll('[class*="w-1.5"]');
      expect(smallDots.length).toBeGreaterThan(0);
    });

    it('should render medium size (default)', () => {
      const { container } = render(<TypingIndicator size="md" />);

      const mediumDots = container.querySelectorAll('[class*="w-2"]');
      expect(mediumDots.length).toBeGreaterThan(0);
    });

    it('should render large size', () => {
      const { container } = render(<TypingIndicator size="lg" />);

      const largeDots = container.querySelectorAll('[class*="w-2.5"]');
      expect(largeDots.length).toBeGreaterThan(0);
    });
  });

  describe('Reduced Motion', () => {
    it('should render static dots when prefersReducedMotion is true', () => {
      const { container } = render(<TypingIndicator prefersReducedMotion />);

      // Should not have animate classes
      const animatedDots = container.querySelectorAll('.animate-bounce, .animate-pulse');
      expect(animatedDots.length).toBe(0);
    });
  });

  describe('Custom Styling', () => {
    it('should accept custom className', () => {
      const { container } = render(<TypingIndicator className="custom-class" />);

      const indicator = container.firstChild;
      expect(indicator).toHaveClass('custom-class');
    });
  });
});