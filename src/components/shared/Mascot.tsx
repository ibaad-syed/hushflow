import React from "react";
import { HushflowMark } from "../icons/HushflowLogo";

interface MascotProps {
  /** Height in px. */
  size: number;
  /** Kept for callers; the mark arrives with one quiet fade-up either way. */
  animation?: "wave" | "slap" | null;
}

/** The app mark where onboarding used to show Handy's mascot. */
export const Mascot: React.FC<MascotProps> = ({ size, animation = null }) => (
  <span
    key={animation ?? "still"}
    className={`block select-none drop-shadow-[0_8px_24px_rgba(0,0,0,0.25)] ${animation ? "animate-rise" : ""}`}
  >
    <HushflowMark size={size} />
  </span>
);
