/** The tray icon, drawn from src-tauri/icons/source/tray/tray_*.svg; uses currentColor like a macOS template image. */
type TrayIconState = "idle" | "recording" | "transcribing";

/** Bar heights for the soundwave, left to right. */
const WAVE: Record<"idle" | "recording", [number, number][]> = {
  idle: [
    [24, 16],
    [17, 30],
    [11, 42],
    [19, 26],
    [25, 14],
  ],
  recording: [
    [22, 20],
    [14, 36],
    [9, 46],
    [16, 32],
    [23, 18],
  ],
};

const Wave = ({ bars }: { bars: [number, number][] }) => (
  <>
    {bars.map(([y, height], i) => (
      <rect key={i} x={8 + i * 8.5} y={y} width={5} height={height} rx={2.5} />
    ))}
  </>
);

/** The text cursor that closes the mark. */
const Cursor = () => <rect x={53.5} y={12} width={4} height={40} rx={2} />;

const TrayIcon = ({
  state,
  size,
}: {
  state: TrayIconState;
  size: number | string;
}) => (
  <svg
    width={size}
    height={size}
    viewBox="0 0 64 64"
    className="fill-current"
    xmlns="http://www.w3.org/2000/svg"
  >
    {state === "idle" && (
      <>
        <Wave bars={WAVE.idle} />
        <Cursor />
      </>
    )}
    {state === "recording" && (
      <>
        <Wave bars={WAVE.recording} />
        <circle cx={55} cy={32} r={7} />
      </>
    )}
    {state === "transcribing" && (
      <>
        {[14, 25, 36].map((cx) => (
          <circle key={cx} cx={cx} cy={32} r={3.6} />
        ))}
        <Cursor />
      </>
    )}
  </svg>
);

export default TrayIcon;
