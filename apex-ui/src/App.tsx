import React from 'react';
import { Workspace } from './components/workspace/Workspace';
import { CommandBar } from './components/workspace/CommandBar';
import { StatusBar } from './components/workspace/StatusBar';
import { useDataSync } from './lib/useDataSync';
import { useTauriEventBridge } from './hooks/useTauriEvents';
import { useWorkspaceStore } from './stores/workspaceStore';

export default function App() {
  useDataSync();
  useTauriEventBridge();

  const setCommandSymbol = useWorkspaceStore((s) => s.setCommandSymbol);
  const setCommandTab = useWorkspaceStore((s) => s.setCommandTab);

  return (
    <div className="flex flex-col h-screen bg-surface-0">
      <CommandBar onSelectSymbol={setCommandSymbol} onSwitchTab={setCommandTab} />
      <main className="flex-1 overflow-hidden">
        <Workspace />
      </main>
      <StatusBar />
    </div>
  );
}
