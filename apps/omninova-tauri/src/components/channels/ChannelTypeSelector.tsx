/**
 * ChannelTypeSelector 组件
 *
 * 显示可添加的渠道类型列表，让用户选择要配置的渠道类型
 *
 * [Source: Story 6.8 - 渠道配置界面]
 */

import { type FC } from 'react';
import {
  MessageSquare,
  Hash,
  Mail,
  Send,
  Webhook,
  Plus,
} from 'lucide-react';
import { Card } from '@/components/ui/card';
import {
  type ChannelKind,
  type ChannelTypeDefinition,
  CHANNEL_TYPE_DEFINITIONS,
} from '@/types/channel';

export interface ChannelTypeSelectorProps {
  /** Callback when a channel type is selected */
  onSelect: (kind: ChannelKind) => void;
  /** Channel counts by kind (for showing configured count) */
  channelCounts?: Record<string, number>;
  /** Filter to show only these channel kinds */
  availableKinds?: ChannelKind[];
  /** Whether the selector is disabled */
  disabled?: boolean;
}

/**
 * Get icon component by name
 */
function getIconComponent(iconName: string, className?: string) {
  const icons: Record<string, React.ReactNode> = {
    MessageSquare: <MessageSquare className={className} />,
    Hash: <Hash className={className} />,
    Mail: <Mail className={className} />,
    Send: <Send className={className} />,
    Webhook: <Webhook className={className} />,
  };
  return icons[iconName] || <MessageSquare className={className} />;
}

/**
 * ChannelTypeCard - Individual channel type card
 */
interface ChannelTypeCardProps {
  definition: ChannelTypeDefinition;
  count: number;
  onSelect: () => void;
  disabled?: boolean;
}

const ChannelTypeCard: FC<ChannelTypeCardProps> = ({
  definition,
  count,
  onSelect,
  disabled,
}) => {
  return (
    <Card
      className={`
        p-4 cursor-pointer transition-all
        hover:border-primary hover:shadow-sm
        ${disabled ? 'opacity-50 cursor-not-allowed' : ''}
      `}
      onClick={() => !disabled && onSelect()}
    >
      <div className="flex flex-col items-center text-center space-y-2">
        {/* Icon */}
        <div className="p-3 rounded-full bg-primary/10 text-primary">
          {getIconComponent(definition.icon, 'h-6 w-6')}
        </div>

        {/* Name */}
        <span className="font-medium">{definition.name}</span>

        {/* Count badge */}
        {count > 0 && (
          <span className="text-xs text-muted-foreground">
            已配置: {count}
          </span>
        )}

        {/* Add indicator */}
        <div className="flex items-center gap-1 text-xs text-muted-foreground">
          <Plus className="h-3 w-3" />
          <span>添加</span>
        </div>
      </div>
    </Card>
  );
};

/**
 * ChannelTypeSelector component
 */
export const ChannelTypeSelector: FC<ChannelTypeSelectorProps> = ({
  onSelect,
  channelCounts = {},
  availableKinds,
  disabled,
}) => {
  // Filter available types
  const displayTypes = availableKinds
    ? CHANNEL_TYPE_DEFINITIONS.filter((def) =>
        availableKinds.includes(def.kind)
      )
    : CHANNEL_TYPE_DEFINITIONS;

  return (
    <div className="space-y-4">
      <div>
        <h3 className="text-lg font-semibold">选择渠道类型</h3>
        <p className="text-sm text-muted-foreground">
          选择要添加的渠道类型，然后配置连接凭据
        </p>
      </div>

      <div className="grid grid-cols-2 sm:grid-cols-3 md:grid-cols-4 gap-4">
        {displayTypes.map((definition) => (
          <ChannelTypeCard
            key={definition.kind}
            definition={definition}
            count={channelCounts[definition.kind] || 0}
            onSelect={() => onSelect(definition.kind)}
            disabled={disabled}
          />
        ))}
      </div>
    </div>
  );
};

export default ChannelTypeSelector;