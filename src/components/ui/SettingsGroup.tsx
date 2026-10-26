import React from "react";

interface SettingsGroupProps {
  title?: string;
  description?: string;
  children: React.ReactNode;
}

export const SettingsGroup: React.FC<SettingsGroupProps> = ({
  title,
  description,
  children,
}) => {
  return (
    <div className="space-y-2">
      {title && (
        <div className="px-4">
          <h2 className="text-[13px] font-semibold text-ink-2">{title}</h2>
          {description && (
            <p className="mt-0.5 text-[12px] text-ink-3">{description}</p>
          )}
        </div>
      )}
      <div className="overflow-visible rounded-[10px] border border-line bg-panel">
        <div className="divide-y divide-line">{children}</div>
      </div>
    </div>
  );
};
