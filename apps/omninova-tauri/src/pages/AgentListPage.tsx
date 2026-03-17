/**
 * AI 代理列表页面
 *
 * 显示所有 AI 代理的列表，支持:
 * - 名称和描述搜索
 * - 人格类型筛选
 * - 创建新代理
 * - 点击导航到对话页面
 *
 * [Source: ux-design-specification.md#页面设计]
 * [Source: 2-6-agent-list-card.md]
 */

import * as React from 'react';
import { useState, useEffect, useCallback, useMemo } from 'react';
import { useNavigate } from 'react-router-dom';
import { invoke } from '@tauri-apps/api/core';
import { toast } from 'sonner';
import { Plus, Search, Users, AlertCircle } from 'lucide-react';
import { AgentList, type AgentModel } from '@/components/agent/AgentList';
import { type MBTIType, personalityColors, allMBTITypes } from '@/lib/personality-colors';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select';
import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
} from '@/components/ui/alert-dialog';

// ============================================================================
// 类型定义
// ============================================================================

/**
 * 筛选状态
 */
interface FilterState {
  /** 搜索词 */
  searchTerm: string;
  /** 选中的人格类型 */
  mbtiType: MBTIType | 'all';
}

// ============================================================================
// 子组件
// ============================================================================

/**
 * 筛选栏组件
 */
interface FilterBarProps {
  searchTerm: string;
  onSearchChange: (value: string) => void;
  mbtiType: MBTIType | 'all';
  onMbtiTypeChange: (value: MBTIType | 'all') => void;
  totalCount: number;
  filteredCount: number;
}

function FilterBar({
  searchTerm,
  onSearchChange,
  mbtiType,
  onMbtiTypeChange,
  totalCount,
  filteredCount,
}: FilterBarProps) {
  return (
    <div className="flex flex-col sm:flex-row items-stretch sm:items-center gap-3">
      {/* 搜索框 */}
      <div className="relative flex-1">
        <Search className="absolute left-3 top-1/2 -translate-y-1/2 w-4 h-4 text-muted-foreground" />
        <Input
          type="text"
          placeholder="搜索代理名称..."
          value={searchTerm}
          onChange={(e) => onSearchChange(e.target.value)}
          className="pl-9"
        />
      </div>

      {/* 人格类型筛选 */}
      <div className="flex items-center gap-2">
        <Select
          value={mbtiType}
          onValueChange={(value) => onMbtiTypeChange(value as MBTIType | 'all')}
        >
          <SelectTrigger className="w-[140px]">
            <SelectValue placeholder="人格类型" />
          </SelectTrigger>
          <SelectContent>
            <SelectItem value="all">全部类型</SelectItem>
            {allMBTITypes.map((type) => (
              <SelectItem key={type} value={type}>
                <span
                  style={{ color: personalityColors[type].primary }}
                  className="font-medium"
                >
                  {type}
                </span>
              </SelectItem>
            ))}
          </SelectContent>
        </Select>
      </div>

      {/* 结果计数 */}
      <div className="text-sm text-muted-foreground whitespace-nowrap">
        共 {filteredCount} 个代理
        {filteredCount !== totalCount && ` (共 ${totalCount} 个)`}
      </div>
    </div>
  );
}

/**
 * 错误状态组件
 */
function ErrorState({ message, onRetry }: { message: string; onRetry?: () => void }) {
  return (
    <div className="flex flex-col items-center justify-center py-16 text-center">
      <AlertCircle className="w-12 h-12 text-destructive/50 mb-4" />
      <h3 className="text-lg font-medium text-foreground/70 mb-2">
        加载失败
      </h3>
      <p className="text-sm text-muted-foreground mb-4">
        {message}
      </p>
      {onRetry && (
        <Button variant="outline" onClick={onRetry}>
          重试
        </Button>
      )}
    </div>
  );
}

// ============================================================================
// 主组件
// ============================================================================

/**
 * AI 代理列表页面
 *
 * @example
 * ```tsx
 * // 在路由中使用
 * <Route path="/agents" element={<AgentListPage />} />
 * ```
 */
export function AgentListPage(): React.ReactElement {
  const navigate = useNavigate();

  // ============================================================================
  // 状态
  // ============================================================================

  const [agents, setAgents] = useState<AgentModel[]>([]);
  const [isLoading, setIsLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [togglingAgentUuid, setTogglingAgentUuid] = useState<string | null>(null);
  const [agentToDelete, setAgentToDelete] = useState<AgentModel | null>(null);
  const [isDeleting, setIsDeleting] = useState(false);
  const [filter, setFilter] = useState<FilterState>({
    searchTerm: '',
    mbtiType: 'all',
  });

  // ============================================================================
  // 数据加载
  // ============================================================================

  const loadAgents = useCallback(async () => {
    setIsLoading(true);
    setError(null);

    try {
      const data = await invoke<AgentModel[]>('get_agents');
      setAgents(data);
    } catch (err) {
      const message = err instanceof Error ? err.message : '加载代理列表失败';
      setError(message);
    } finally {
      setIsLoading(false);
    }
  }, []);

  useEffect(() => {
    loadAgents();
  }, [loadAgents]);

  // ============================================================================
  // 筛选逻辑
  // ============================================================================

  const filteredAgents = useMemo(() => {
    return agents.filter((agent) => {
      // 名称/描述搜索
      const matchesSearch =
        !filter.searchTerm ||
        agent.name.toLowerCase().includes(filter.searchTerm.toLowerCase()) ||
        agent.description?.toLowerCase().includes(filter.searchTerm.toLowerCase());

      // 人格类型筛选
      const matchesMbti =
        filter.mbtiType === 'all' || agent.mbti_type === filter.mbtiType;

      return matchesSearch && matchesMbti;
    });
  }, [agents, filter]);

  // ============================================================================
  // 事件处理
  // ============================================================================

  const handleAgentClick = useCallback(
    (agent: AgentModel) => {
      navigate(`/agents/${agent.agent_uuid}/chat`);
    },
    [navigate]
  );

  const handleCreateAgent = useCallback(() => {
    navigate('/agents/create');
  }, [navigate]);

  const handleSearchChange = useCallback((value: string) => {
    setFilter((prev) => ({ ...prev, searchTerm: value }));
  }, []);

  const handleMbtiTypeChange = useCallback((value: MBTIType | 'all') => {
    setFilter((prev) => ({ ...prev, mbtiType: value }));
  }, []);

  const handleEditAgent = useCallback(
    (agent: AgentModel) => {
      navigate(`/agents/${agent.agent_uuid}/edit`);
    },
    [navigate]
  );

  const handleDuplicateAgent = useCallback(
    async (agent: AgentModel) => {
      try {
        const duplicatedJson = await invoke<string>('duplicate_agent', {
          uuid: agent.agent_uuid,
        });
        const duplicated = JSON.parse(duplicatedJson) as AgentModel;
        toast.success(`已创建副本: ${duplicated.name}`);
        // 刷新列表以确保用户返回时能看到新代理
        await loadAgents();
        navigate(`/agents/${duplicated.agent_uuid}/edit`);
      } catch (error) {
        const message =
          error instanceof Error ? error.message : '复制代理失败';
        toast.error(message);
      }
    },
    [navigate, loadAgents]
  );

  const handleToggleAgent = useCallback(
    async (agent: AgentModel) => {
      // 防止重复点击
      if (togglingAgentUuid) return;

      // 切换状态：active <-> inactive
      const newStatus = agent.status === 'active' ? 'inactive' : 'active';
      setTogglingAgentUuid(agent.agent_uuid);
      try {
        const updates = { status: newStatus };
        await invoke<string>('update_agent', {
          uuid: agent.agent_uuid,
          updatesJson: JSON.stringify(updates),
        });
        toast.success(newStatus === 'active' ? `已启用: ${agent.name}` : `已停用: ${agent.name}`);
        // 刷新列表以更新状态显示
        await loadAgents();
      } catch (error) {
        const message =
          error instanceof Error ? error.message : '切换状态失败';
        toast.error(message);
      } finally {
        setTogglingAgentUuid(null);
      }
    },
    [loadAgents, togglingAgentUuid]
  );

  // 打开删除确认对话框
  const handleDeleteAgent = useCallback(
    (agent: AgentModel) => {
      setAgentToDelete(agent);
    },
    []
  );

  // 取消删除
  const handleCancelDelete = useCallback(() => {
    setAgentToDelete(null);
  }, []);

  // 确认删除
  const handleConfirmDelete = useCallback(
    async () => {
      if (!agentToDelete || isDeleting) return;

      setIsDeleting(true);
      try {
        await invoke('delete_agent', { uuid: agentToDelete.agent_uuid });
        toast.success(`已删除: ${agentToDelete.name}`);
        setAgentToDelete(null);
        // 刷新列表
        await loadAgents();
      } catch (error) {
        const message =
          error instanceof Error ? error.message : '删除代理失败';
        toast.error(message);
      } finally {
        setIsDeleting(false);
      }
    },
    [agentToDelete, isDeleting, loadAgents]
  );

  // ============================================================================
  // 渲染
  // ============================================================================

  return (
    <div className="min-h-screen bg-background">
      {/* 页面头部 */}
      <header className="sticky top-0 z-10 border-b border-border/50 bg-background/95 backdrop-blur supports-[backdrop-filter]:bg-background/60">
        <div className="container flex items-center justify-between h-16 px-4">
          <div className="flex items-center gap-2">
            <Users className="w-5 h-5 text-primary" />
            <h1 className="text-xl font-semibold">我的代理</h1>
          </div>
          <Button onClick={handleCreateAgent}>
            <Plus className="w-4 h-4 mr-2" />
            创建新代理
          </Button>
        </div>
      </header>

      {/* 主要内容区域 */}
      <main className="container px-4 py-6">
        {/* 筛选栏 */}
        {!error && agents.length > 0 && (
          <div className="mb-6">
            <FilterBar
              searchTerm={filter.searchTerm}
              onSearchChange={handleSearchChange}
              mbtiType={filter.mbtiType}
              onMbtiTypeChange={handleMbtiTypeChange}
              totalCount={agents.length}
              filteredCount={filteredAgents.length}
            />
          </div>
        )}

        {/* 代理列表 */}
        {error ? (
          <ErrorState message={error} onRetry={loadAgents} />
        ) : (
          <AgentList
            agents={filteredAgents}
            isLoading={isLoading}
            onAgentClick={handleAgentClick}
            onCreateAgent={agents.length === 0 ? handleCreateAgent : undefined}
            showEditButton
            onEdit={handleEditAgent}
            showDuplicateButton
            onDuplicate={handleDuplicateAgent}
            showToggleButton
            onToggle={handleToggleAgent}
            showDeleteButton
            onDelete={handleDeleteAgent}
          />
        )}
      </main>

      {/* 删除确认对话框 */}
      <AlertDialog open={!!agentToDelete} onOpenChange={(open) => !open && handleCancelDelete()}>
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle>确认删除</AlertDialogTitle>
            <AlertDialogDescription>
              确定要删除代理 "{agentToDelete?.name}" 吗？
              <br />
              <span className="text-destructive">此操作不可撤销，该代理将被永久删除。</span>
            </AlertDialogDescription>
          </AlertDialogHeader>
          <AlertDialogFooter>
            <AlertDialogCancel disabled={isDeleting}>取消</AlertDialogCancel>
            <AlertDialogAction
              onClick={handleConfirmDelete}
              disabled={isDeleting}
              className="bg-destructive text-destructive-foreground hover:bg-destructive/90"
            >
              {isDeleting ? '删除中...' : '删除'}
            </AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>
    </div>
  );
}

export default AgentListPage;