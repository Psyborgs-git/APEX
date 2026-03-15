import React from 'react';
import { Workspace } from './components/workspace/Workspace';
import { CommandBar } from './components/workspace/CommandBar';
import { StatusBar } from './components/workspace/StatusBar';

export default function App() {
  return (
    <div className="flex flex-col h-screen bg-surface-0">
      <CommandBar />
      <main className="flex-1 overflow-hidden">
        <Workspace />
      </main>
      <StatusBar />
    </div>
  );
}
