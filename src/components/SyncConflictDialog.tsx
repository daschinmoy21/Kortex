import { Dialog, DialogPanel, DialogTitle } from '@headlessui/react';
import { X, AlertTriangle, FolderOpen } from 'lucide-react';
import { invoke } from '@tauri-apps/api/core';
import { openPath } from '@tauri-apps/plugin-opener';

interface Props {
    isOpen: boolean;
    onClose: () => void;
    conflictMessage: string | null;
    onForcePull: () => void;
    onForcePush: () => void;
}

export function SyncConflictDialog({ isOpen, onClose, conflictMessage }: Props) {
    const handleOpenFolder = async () => {
        try {
            const notesPath = await invoke<string>('get_notes_path');
            // Open the parent (Logia) folder
            const parentPath = notesPath.replace(/\/notes\/?$/, '') || notesPath;
            await openPath(parentPath);
        } catch (e) {
            console.error('Failed to open folder', e);
        }
    };

    return (
        <Dialog open={isOpen} onClose={onClose} className="relative z-[1000]">
            <div className="fixed inset-0 bg-black/40" aria-hidden="true" />

            <div className="fixed inset-0 flex items-center justify-center p-4">
                <DialogPanel className="w-full max-w-sm rounded-lg bg-zinc-900 border border-zinc-800 shadow-lg">
                    {/* Header */}
                    <div className="flex items-center justify-between px-4 py-3 border-b border-zinc-800">
                        <DialogTitle className="text-sm font-medium text-zinc-100 flex items-center gap-2">
                            <AlertTriangle size={16} className="text-amber-400" />
                            Sync Conflict
                        </DialogTitle>
                        <button onClick={onClose} className="text-zinc-500 hover:text-zinc-300">
                            <X size={16} />
                        </button>
                    </div>

                    {/* Content */}
                    <div className="px-4 py-4 space-y-4">
                        <p className="text-sm text-zinc-300">
                            {conflictMessage || 'A sync conflict was detected. The remote has changes that conflict with your local changes.'}
                        </p>
                        <p className="text-xs text-zinc-500">
                            You can resolve this manually by opening your notes folder and using git commands,
                            or use one of the force options in Settings.
                        </p>
                        <button
                            onClick={handleOpenFolder}
                            className="w-full flex items-center justify-center gap-2 px-4 py-2 rounded-lg text-sm font-medium bg-zinc-700 text-zinc-200 hover:bg-zinc-600 transition-all"
                        >
                            <FolderOpen size={16} />
                            Open Notes Folder
                        </button>
                    </div>

                    {/* Footer */}
                    <div className="flex justify-end gap-2 px-4 py-3 border-t border-zinc-800">
                        <button
                            onClick={onClose}
                            className="px-3 py-1.5 text-xs text-zinc-400 hover:text-zinc-200 transition-colors"
                        >
                            Close
                        </button>
                    </div>
                </DialogPanel>
            </div>
        </Dialog>
    );
}
