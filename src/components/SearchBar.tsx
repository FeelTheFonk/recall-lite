import { Search, FolderPlus, Loader2 } from "lucide-react";

interface SearchBarProps {
    query: string;
    onQueryChange: (value: string) => void;
    activeContainer: string;
    isIndexing: boolean;
    onPickFolder: () => void;
    inputRef: React.RefObject<HTMLInputElement | null>;
}

export default function SearchBar({
    query, onQueryChange, activeContainer, isIndexing, onPickFolder, inputRef,
}: SearchBarProps) {
    return (
        <div className="search-wrapper shrink-0">
            <div className="relative">
                <Search className="absolute left-4 top-1/2 -translate-y-1/2 text-[--color-text-tertiary] pointer-events-none" size={18} />
                <input
                    ref={inputRef}
                    type="text"
                    value={query}
                    onChange={(e) => onQueryChange(e.target.value)}
                    placeholder={`Search in ${activeContainer}...`}
                    className="search-input"
                    autoFocus
                />
                <button
                    onClick={onPickFolder}
                    className="absolute right-4 top-1/2 -translate-y-1/2 p-2 rounded-md hover:bg-[--color-control-fill-secondary] text-[--color-text-secondary] transition-colors"
                    title={`Index Folder into ${activeContainer} (Ctrl+O)`}
                >
                    {isIndexing ? <Loader2 className="animate-spin" size={18} /> : <FolderPlus size={18} />}
                </button>
            </div>
        </div>
    );
}
