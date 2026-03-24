/**
 * Import/Export Settings Page
 *
 * Page for managing configuration import and export.
 * [Source: Story 7.8 - 配置导入导出功能]
 */

import { useState, useEffect, useCallback } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { Button } from '@/components/ui/button';
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from '@/components/ui/card';
import { Download, Upload, FileJson } from 'lucide-react';
import {
  ConfigExportDialog,
  ConfigImportDialog,
} from '@/components/configuration';
import type { AgentModel } from '@/types/agent';
import type { ImportResult } from '@/types/config-import-export';

export function ImportExportPage() {
  const [exportDialogOpen, setExportDialogOpen] = useState(false);
  const [importDialogOpen, setImportDialogOpen] = useState(false);
  const [agents, setAgents] = useState<AgentModel[]>([]);

  const loadAgents = useCallback(async () => {
    try {
      const data = await invoke<AgentModel[]>('get_agents');
      setAgents(data);
    } catch (error) {
      console.error('Failed to load agents:', error);
    }
  }, []);

  useEffect(() => {
    loadAgents();
  }, [loadAgents]);

  const handleImportComplete = async (result: ImportResult) => {
    if (result.success) {
      // Refresh agent list
      await loadAgents();
    }
  };

  return (
    <div className="space-y-6">
      <div>
        <h2 className="text-2xl font-bold tracking-tight">导入导出</h2>
        <p className="text-muted-foreground">
          备份代理配置或从文件恢复配置
        </p>
      </div>

      <div className="grid gap-6 md:grid-cols-2">
        {/* Export Card */}
        <Card>
          <CardHeader>
            <CardTitle className="flex items-center gap-2">
              <Download className="h-5 w-5" />
              导出配置
            </CardTitle>
            <CardDescription>
              将代理配置导出为 JSON 或 YAML 文件
            </CardDescription>
          </CardHeader>
          <CardContent className="space-y-4">
            <p className="text-sm text-muted-foreground">
              导出包含代理的基本信息、人格配置、技能配置等内容。
              敏感信息（如 API 密钥）不会被导出。
            </p>
            <div className="flex items-center gap-2 text-sm">
              <FileJson className="h-4 w-4 text-muted-foreground" />
              <span>支持格式: JSON, YAML</span>
            </div>
            <Button
              onClick={() => setExportDialogOpen(true)}
              disabled={agents.length === 0}
              className="w-full"
            >
              <Download className="mr-2 h-4 w-4" />
              导出配置
            </Button>
          </CardContent>
        </Card>

        {/* Import Card */}
        <Card>
          <CardHeader>
            <CardTitle className="flex items-center gap-2">
              <Upload className="h-5 w-5" />
              导入配置
            </CardTitle>
            <CardDescription>
              从文件导入代理配置
            </CardDescription>
          </CardHeader>
          <CardContent className="space-y-4">
            <p className="text-sm text-muted-foreground">
              导入前会验证文件格式和版本兼容性。可以选择覆盖现有配置或合并导入。
            </p>
            <div className="flex items-center gap-2 text-sm">
              <FileJson className="h-4 w-4 text-muted-foreground" />
              <span>支持格式: JSON, YAML</span>
            </div>
            <Button
              onClick={() => setImportDialogOpen(true)}
              className="w-full"
            >
              <Upload className="mr-2 h-4 w-4" />
              导入配置
            </Button>
          </CardContent>
        </Card>
      </div>

      {/* Tips Card */}
      <Card>
        <CardHeader>
          <CardTitle>提示</CardTitle>
        </CardHeader>
        <CardContent>
          <ul className="list-disc list-inside space-y-2 text-sm text-muted-foreground">
            <li>导出前确保所有重要配置已保存</li>
            <li>导入配置前建议先备份现有配置</li>
            <li>敏感信息（API 密钥、Token）需要手动重新配置</li>
            <li>不同主版本号之间的配置可能不兼容</li>
          </ul>
        </CardContent>
      </Card>

      {/* Export Dialog */}
      <ConfigExportDialog
        open={exportDialogOpen}
        onOpenChange={setExportDialogOpen}
        agents={agents}
      />

      {/* Import Dialog */}
      <ConfigImportDialog
        open={importDialogOpen}
        onOpenChange={setImportDialogOpen}
        onImportComplete={handleImportComplete}
      />
    </div>
  );
}