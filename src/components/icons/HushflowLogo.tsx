import React from "react";

/** The product name is never translated. */
export const BRAND = "Hushflow";

/** The app mark: a soundwave that ends in a text cursor (voice becomes text).
 *  Same drawing as src-tauri/icons/source/app-icon.svg, cropped to the tile. */
export const HushflowMark: React.FC<{ size?: number; className?: string }> = ({
  size = 24,
  className,
}) => (
  <svg
    width={size}
    height={size}
    viewBox="100 100 824 824"
    className={className}
    aria-hidden="true"
  >
    <defs>
      <linearGradient id="hf-bg" x1="0" y1="0" x2="1" y2="1">
        <stop offset="0" stopColor="#1E1B4B" />
        <stop offset="1" stopColor="#0B0A1F" />
      </linearGradient>
      <linearGradient
        id="hf-wave"
        gradientUnits="userSpaceOnUse"
        x1="239"
        y1="277"
        x2="679"
        y2="747"
      >
        <stop offset="0" stopColor="#5EEAD4" />
        <stop offset="1" stopColor="#A78BFA" />
      </linearGradient>
    </defs>
    <rect
      x="100"
      y="100"
      width="824"
      height="824"
      rx="185"
      fill="url(#hf-bg)"
    />
    <g fill="url(#hf-wave)">
      <rect x="239" y="422" width="56" height="180" rx="28" />
      <rect x="335" y="342" width="56" height="340" rx="28" />
      <rect x="431" y="277" width="56" height="470" rx="28" />
      <rect x="527" y="362" width="56" height="300" rx="28" />
      <rect x="623" y="432" width="56" height="160" rx="28" />
    </g>
    <rect x="749" y="302" width="36" height="420" rx="18" fill="#FFFFFF" />
  </svg>
);

/** Mark and wordmark, for the sidebar and About. */
export const HushflowLogo: React.FC<{ className?: string }> = ({
  className = "",
}) => (
  <div className={`flex items-center gap-2 ${className}`}>
    <HushflowMark size={22} />
    <span className="text-[15px] font-semibold tracking-[-0.01em] text-text">
      {BRAND}
    </span>
  </div>
);
