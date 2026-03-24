/**
 * ConfigImportDialog Component
 *
 * Dialog for importing agent configurations.
 * [Source: Story 7.8 - 配置导入导出功能]
 */

import { useState, useEffect, useCallback } from 'react';
import { toast } from 'sonner';
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog';
import { Button } from '@/components/ui/button';
import { Label } from '@/components/ui/label';
import { RadioGroup, RadioGroupItem } from '@/components/ui/radio-group';
import { Alert, AlertDescription } from '@/components/ui/alert';
import {
  Upload,
  FileJson,
  CheckCircle,
  XCircle,
  AlertTriangle,
  Loader2,
  FolderOpen,
} from 'lucide-react';
import {
  type ImportOptions,
  type ImportValidationResult,
  type ImportResult,
  DEFAULT_IMPORT_OPTIONS,
  IMPORT_STRATEGY_LABELS,
  IMPORT_STRATEGY_DESCRIPTIONS,
  type ImportStrategy,
} from '@/types/config-import-export';
import { useConfigImportExport } from '@/hooks/useConfigImportExport';
import { open } from '@tauri-apps/plugin-dialog';

export interface ConfigImportDialogProps {
  /** Dialog open state */
  open: boolean;
  /** Open state change callback */
  onOpenChange: (open: boolean) => void;
  /** Import complete callback */
  onImportComplete?: (result: ImportResult) => void;
}

export function ConfigImportDialog({
  open,
  onOpenChange,
  onImportComplete,
}: ConfigImportDialogProps) {
  const { importState, validateImportFile, importConfig, resetImportState } =
    useConfigImportExport();

  const [selectedFile, setSelectedFile] = useState<string | null>(null);
  const [options, setOptions] = useState<ImportOptions>(DEFAULT_IMPORT_OPTIONS);
  const [validationResult, setValidationResult] =
    useState<ImportValidationResult | null>(null);
  const [importResult, setImportResult] = useState<ImportResult | null>(null);

  // Reset state when dialog opens
  useEffect(() => {
    if (open) {
      resetImportState();
      setSelectedFile(null);
      setValidationResult(null);
      setImportResult(null);
      setOptions(DEFAULT_IMPORT_OPTIONS);
    }
  }, [open, resetImportState]);

  const handleSelectFile = async () => {
    const filePath = await open({
      multiple: false,
      filters: [
        { name: 'JSON', extensions: ['json'] },
        { name: 'YAML', extensions: ['yaml', 'yml'] },
        { name: 'All Files', extensions: ['*'] },
      ],
    });

    if (filePath && typeof filePath === 'string') {
      setSelectedFile(filePath);
      setValidationResult(null);
      setImportResult(null);

      // Validate the file
      const result = await validateImportFile(filePath);
      if (result) {
        setValidationResult(result);
      }
    }
  };

  const handleImport = async () => {
    if (!selectedFile) return;

    const result = await importConfig(selectedFile, options);
    if (result) {
      setImportResult(result);
      if (result.success) {
        toast.success('导入成功', {
          description: `已导入 ${result.importedCount} 个代理配置`,
        });
        onImportComplete?.(result);
      }
    }
  };

  const handleStrategyChange = (strategy: string) => {
    setOptions(prev => ({
      ...prev,
      strategy: strategy as ImportStrategy,
      overwriteExisting: strategy === 'overwrite',
    }));
  };

  const getFileName = (path: string) => {
    return path.split(/[/\\]/).pop() || path;
  };

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="sm:max-w-[550px]">
        <DialogHeader>
          <DialogTitle className="flex items-center gap-2">
            <Upload className="h-5 w-5" />
            导入配置
          </DialogTitle>
          <DialogDescription>
            从文件导入代理配置，支持 JSON 和 YAML 格式
          </DialogDescription>
        </DialogHeader>

        <div className="space-y-6 py-4">
          {/* File Selection */}
          <div className="space-y-3">
            <Label className="text-base">选择文件</Label>
            <div
              className="flex items-center gap-3 p-4 border rounded-lg cursor-pointer hover:bg-accent/50 transition-colors"
              onClick={handleSelectFile}
            >
              {selectedFile ? (
                <>
                  <FileJson className="h-8 w-8 text-muted-foreground" />
                  <div className="flex-1 min-w-0">
                    <p className="font-medium truncate">{getFileName(selectedFile)}</p>
                    <p className="text-sm text-muted-foreground">
                      {validationResult?.format.toUpperCase() || '检测格式中...'}
                    </p>
                  </div>
                </>
              ) : (
                <>
                  <FolderOpen className="h-8 w-8 text-muted-foreground" />
                  <div className="flex-1">
                    <p className="text-muted-foreground">点击选择文件</p>
                    <p className="text-sm text-muted-foreground">支持 .json, .yaml, .yml</p>
                  </div>
                </>
              )}
              <Button variant="outline" size="sm" onClick={(e) => { e.stopPropagation(); handleSelectFile(); }}>
                选择
              </Button>
            </div>
          </div>

          {/* Validation Preview */}
          {validationResult && (
            <div className="space-y-3">
              <Label className="text-base">文件内容预览</Label>
              <div className="p-4 border rounded-lg space-y-2">
                <div className="flex items-center gap-2">
                  <span className="text-sm text-muted-foreground">版本:</span>
                  <span className="text-sm font-medium">{validationResult.format.toUpperCase()}</span>
                </div>
                <div className="flex items-center gap-2">
                  <span className="text-sm text-muted-foreground">代理数量:</span>
                  <span className="text-sm font-medium">{validationResult.agentCount}</span>
                </div>

                {/* Validation Status */}
                <div className="flex items-center gap-2">
                  {validationResult.valid ? (
                    <>
                      <CheckCircle className="h-4 w-4 text-green-500" />
                      <span className="text-sm text-green-600">格式验证通过</span>
                    </>
                  ) : (
                    <>
                      <XCircle className="h-4 w-4 text-destructive" />
                      <span className="text-sm text-destructive">格式验证失败</span>
                    </>
                  )}
                </div>

                <div className="flex items-center gap-2">
                  {validationResult.versionCompatible ? (
                    <>
                      <CheckCircle className="h-4 w-4 text-green-500" />
                      <span className="text-sm text-green-600">版本兼容</span>
                    </>
                  ) : (
                    <>
                      <AlertTriangle className="h-4 w-4 text-yellow-500" />
                      <span className="text-sm text-yellow-600">版本可能不兼容</span>
                    </>
                  )}
                </div>

                {/* Errors */}
                {validationResult.errors.length > 0 && (
                  <Alert variant="destructive" className="mt-2">
                    <AlertDescription>
                      <ul className="list-disc list-inside space-y-1">
                        {validationResult.errors.map((error, index) => (
                          <li key={index} className="text-sm">{error}</li>
                        ))}
                      </ul>
                    </AlertDescription>
                  </Alert>
                )}

                {/* Warnings */}
                {validationResult.warnings.length > 0 && (
                  <Alert className="mt-2 border-yellow-200 bg-yellow-50">
                    <AlertTriangle className="h-4 w-4 text-yellow-500" />
                    <AlertDescription>
                      <ul className="list-disc list-inside space-y-1">
                        {validationResult.warnings.map((warning, index) => (
                          <li key={index} className="text-sm">{warning}</li>
                        ))}
                      </ul>
                    </AlertDescription>
                  </Alert>
                )}
              </div>
            </div>
          )}

          {/* Import Strategy */}
          {validationResult?.valid && (
            <div className="space-y-3">
              <Label className="text-base">导入策略</Label>
              <RadioGroup
                value={options.strategy}
                onValueChange={handleStrategyChange}
                disabled={importState.isImporting}
              >
                {(['merge', 'overwrite'] as ImportStrategy[]).map(strategy => (
                  <div key={strategy} className="flex items-start space-x-2">
                    <RadioGroupItem value={strategy} id={`strategy-${strategy}`} />
                    <div className="grid gap-1 leading-none">
                      <Label htmlFor={`strategy-${strategy}`} className="font-medium">
                        {IMPORT_STRATEGY_LABELS[strategy]}
                      </Label>
                      <p className="text-sm text-muted-foreground">
                        {IMPORT_STRATEGY_DESCRIPTIONS[strategy]}
                      </p>
                    </div>
                  </div>
                ))}
              </RadioGroup>
            </div>
          )}

          {/* Import Result */}
          {importResult && (
            <Alert variant={importResult.success ? 'default' : 'destructive'}>
              {importResult.success ? (
                <CheckCircle className="h-4 w-4" />
              ) : (
                <XCircle className="h-4 w-4" />
              )}
              <AlertDescription>
                <p className="font-medium">
                  {importResult.success ? '导入完成' : '导入失败'}
                </p>
                <p className="text-sm">
                  导入: {importResult.importedCount} 个，跳过: {importResult.skippedCount} 个
                </p>
                {importResult.errors.length > 0 && (
                  <ul className="list-disc list-inside text-sm mt-2">
                    {importResult.errors.slice(0, 5).map((error, index) => (
                      <li key={index}>{error}</li>
                    ))}
                  </ul>
                )}
              </AlertDescription>
            </Alert>
          )}

          {/* Error Display */}
          {importState.error && (
            <Alert variant="destructive">
              <AlertDescription>{importState.error}</AlertDescription>
            </Alert>
          )}
        </div>

        <DialogFooter>
          <Button
            variant="outline"
            onClick={() => onOpenChange(false)}
            disabled={importState.isImporting}
          >
            {importResult?.success ? '完成' : '取消'}
          </Button>
          {!importResult?.success && (
            <Button
              onClick={handleImport}
              disabled={!validationResult?.valid || importState.isImporting || !selectedFile}
            >
              {importState.isImporting ? (
                <>
                  <Loader2 className="mr-2 h-4 w-4 animate-spin" />
                  导入中...
                </>
              ) : (
                <>
                  <Upload className="mr-2 h-4 w-4" />
                  导入
                </>
              )}
            </Button>
          )}
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}