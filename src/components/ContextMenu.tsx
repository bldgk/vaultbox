import { useEffect, useRef, useState } from "react";

export interface ContextMenuItem {
  label: string;
  icon?: React.ReactNode;
  onClick?: () => void;
  danger?: boolean;
  divider?: boolean;
  children?: ContextMenuItem[];
}

interface ContextMenuProps {
  x: number;
  y: number;
  items: ContextMenuItem[];
  onClose: () => void;
}

export function ContextMenu({ x, y, items, onClose }: ContextMenuProps) {
  const ref = useRef<HTMLDivElement>(null);

  useEffect(() => {
    const handler = (e: MouseEvent) => {
      if (ref.current && !ref.current.contains(e.target as Node)) {
        onClose();
      }
    };
    const keyHandler = (e: KeyboardEvent) => {
      if (e.key === "Escape") onClose();
    };
    document.addEventListener("mousedown", handler);
    document.addEventListener("keydown", keyHandler);
    return () => {
      document.removeEventListener("mousedown", handler);
      document.removeEventListener("keydown", keyHandler);
    };
  }, [onClose]);

  // Adjust position so menu stays on screen
  const style: React.CSSProperties = {
    position: "fixed",
    left: x,
    top: y,
    zIndex: 100,
  };

  return (
    <div ref={ref} style={style} className="bg-gray-800 border border-gray-700 rounded-lg shadow-xl py-1 min-w-[160px]">
      {items.map((item, i) => (
        <div key={i}>
          {item.divider && <div className="border-t border-gray-700 my-1" />}
          {item.children ? (
            <SubMenuItem item={item} onClose={onClose} />
          ) : (
            <button
              className={`w-full px-3 py-1.5 text-left text-sm flex items-center gap-2 ${
                item.danger
                  ? "text-red-400 hover:bg-red-900/30"
                  : "text-gray-300 hover:bg-gray-700"
              }`}
              onClick={() => {
                item.onClick?.();
                onClose();
              }}
            >
              {item.icon}
              {item.label}
            </button>
          )}
        </div>
      ))}
    </div>
  );
}

function SubMenuItem({ item, onClose }: { item: ContextMenuItem; onClose: () => void }) {
  const [open, setOpen] = useState(false);
  const containerRef = useRef<HTMLDivElement>(null);
  const timeoutRef = useRef<ReturnType<typeof setTimeout>>(null);

  const handleEnter = () => {
    if (timeoutRef.current) clearTimeout(timeoutRef.current);
    setOpen(true);
  };

  const handleLeave = () => {
    timeoutRef.current = setTimeout(() => setOpen(false), 150);
  };

  // Position the submenu: try right side, flip left if it would overflow
  const getSubStyle = (): React.CSSProperties => {
    const el = containerRef.current;
    if (!el) return { left: "100%", top: 0 };
    const rect = el.getBoundingClientRect();
    const subWidth = 180;
    const flipX = rect.right + subWidth > window.innerWidth;
    const maxBottom = window.innerHeight - 8;
    const subHeight = (item.children?.length ?? 0) * 32 + 8;
    const flipY = rect.top + subHeight > maxBottom;
    return {
      position: "absolute",
      [flipX ? "right" : "left"]: "100%",
      top: flipY ? undefined : 0,
      bottom: flipY ? 0 : undefined,
      marginLeft: flipX ? 0 : 2,
      marginRight: flipX ? 2 : 0,
    };
  };

  return (
    <div
      ref={containerRef}
      className="relative"
      onMouseEnter={handleEnter}
      onMouseLeave={handleLeave}
    >
      <div className="w-full px-3 py-1.5 text-left text-sm flex items-center gap-2 text-gray-300 hover:bg-gray-700 cursor-default">
        {item.icon}
        <span className="flex-1">{item.label}</span>
        <svg className="w-3 h-3 text-gray-500" fill="none" viewBox="0 0 24 24" stroke="currentColor">
          <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M9 5l7 7-7 7" />
        </svg>
      </div>
      {open && item.children && item.children.length > 0 && (
        <div
          style={getSubStyle()}
          className="bg-gray-800 border border-gray-700 rounded-lg shadow-xl py-1 min-w-[160px] max-h-[240px] overflow-y-auto z-[101]"
          onMouseEnter={handleEnter}
          onMouseLeave={handleLeave}
        >
          {item.children.map((child, j) => (
            <button
              key={j}
              className="w-full px-3 py-1.5 text-left text-sm flex items-center gap-2 text-gray-300 hover:bg-gray-700"
              onClick={() => {
                child.onClick?.();
                onClose();
              }}
            >
              <svg className="w-3.5 h-3.5 text-indigo-400 shrink-0" fill="currentColor" viewBox="0 0 24 24">
                <path d="M10 4H4a2 2 0 00-2 2v12a2 2 0 002 2h16a2 2 0 002-2V8a2 2 0 00-2-2h-8l-2-2z" />
              </svg>
              {child.label}
            </button>
          ))}
        </div>
      )}
    </div>
  );
}
