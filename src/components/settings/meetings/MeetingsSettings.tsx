import React, {
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
} from "react";
import { listen } from "@tauri-apps/api/event";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import { ArrowLeft, Copy, Mic, RotateCcw, Search, Trash2 } from "lucide-react";
import {
  commands,
  type ActiveMeeting,
  type MeetingDetail,
  type MeetingSegment,
  type MeetingSummary,
} from "@/bindings";
import { Button } from "../../ui/Button";
import { Input } from "../../ui/Input";
import { Spinner } from "../../ui/Spinner";
import { ToggleSwitch } from "../../ui/ToggleSwitch";
import { SettingsGroup } from "../../ui/SettingsGroup";
import { MarkdownContent } from "../../whats-new/MarkdownContent";
import { useSettings } from "../../../hooks/useSettings";
import { copyToClipboard } from "../history/clipboard";

/** The `meeting-segment` event (managers/meeting/mod.rs). */
interface MeetingSegmentEvent {
  meeting_id: number;
  segment: MeetingSegment;
  /** Earlier mic lines this one showed were echo, now hidden. */
  suppressed: number[];
}

const clock = (ms: number) => {
  const s = Math.floor(ms / 1000);
  const mm = String(Math.floor(s / 60) % 60).padStart(2, "0");
  const ss = String(s % 60).padStart(2, "0");
  return s >= 3600 ? `${Math.floor(s / 3600)}:${mm}:${ss}` : `${mm}:${ss}`;
};

const formatDate = (ms: number) =>
  new Date(ms).toLocaleString(undefined, {
    month: "short",
    day: "numeric",
    hour: "numeric",
    minute: "2-digit",
  });

/** Ticks once a second while `since` is set: the recording timer. */
const useElapsed = (since: number | null) => {
  const [now, setNow] = useState(Date.now());
  useEffect(() => {
    if (since === null) return;
    const id = window.setInterval(() => setNow(Date.now()), 1000);
    return () => window.clearInterval(id);
  }, [since]);
  return since === null ? 0 : Math.max(0, now - since);
};

const Transcript: React.FC<{ segments: MeetingSegment[]; live?: boolean }> = ({
  segments,
  live,
}) => {
  const { t } = useTranslation();
  const shown = segments.filter((s) => !s.suppressed_echo);
  const hidden = segments.length - shown.length;
  const endRef = useRef<HTMLDivElement>(null);
  useEffect(() => {
    if (live) endRef.current?.scrollIntoView({ block: "nearest" });
  }, [live, shown.length]);

  if (shown.length === 0) {
    return (
      <p className="px-4 py-6 text-center text-sm text-ink-3">
        {live
          ? t("meetings.transcript.listening")
          : t("meetings.transcript.empty")}
      </p>
    );
  }
  return (
    <div className="flex flex-col gap-2 px-4 py-3">
      {shown.map((s) => {
        const me = s.channel === "mic";
        return (
          <div
            key={s.id}
            className={`flex gap-3 text-sm ${me ? "" : "flex-row"}`}
          >
            <span className="w-12 shrink-0 pt-0.5 text-xs tabular-nums text-ink-3">
              {clock(s.t0_ms)}
            </span>
            <span
              className={`w-12 shrink-0 pt-0.5 text-xs font-semibold ${me ? "text-logo-primary" : "text-ink-2"}`}
            >
              {me ? t("meetings.speaker.me") : t("meetings.speaker.them")}
            </span>
            <p className="m-0 min-w-0 flex-1 select-text leading-relaxed">
              {s.text}
            </p>
          </div>
        );
      })}
      {hidden > 0 && (
        <p className="m-0 pt-1 text-xs text-ink-3">
          {t("meetings.transcript.echoHidden", { count: hidden })}
        </p>
      )}
      <div ref={endRef} />
    </div>
  );
};

/** The user's own notes, saved as they type. */
const Notepad: React.FC<{ meetingId: number; initial: string }> = ({
  meetingId,
  initial,
}) => {
  const { t } = useTranslation();
  const [body, setBody] = useState(initial);
  const timer = useRef<number | undefined>(undefined);
  const latest = useRef(initial);

  useEffect(() => {
    setBody(initial);
    latest.current = initial;
  }, [meetingId, initial]);

  const save = useCallback(
    (text: string) => commands.saveMeetingNotes(meetingId, text),
    [meetingId],
  );

  // Flush on leave, so a quick Stop never loses the last words.
  useEffect(
    () => () => {
      window.clearTimeout(timer.current);
      void save(latest.current);
    },
    [save],
  );

  return (
    <textarea
      value={body}
      onChange={(e) => {
        const text = e.target.value;
        setBody(text);
        latest.current = text;
        window.clearTimeout(timer.current);
        timer.current = window.setTimeout(() => void save(text), 600);
      }}
      placeholder={t("meetings.notes.placeholder")}
      className="min-h-[160px] w-full resize-y rounded-lg border border-line bg-background px-4 py-3 text-sm leading-relaxed focus:border-logo-primary focus:outline-none"
    />
  );
};

type Tab = "notes" | "mine" | "transcript";

const MeetingView: React.FC<{
  id: number;
  active: ActiveMeeting | null;
  onBack: () => void;
}> = ({ id, active, onBack }) => {
  const { t } = useTranslation();
  const [detail, setDetail] = useState<MeetingDetail | null>(null);
  const [tab, setTab] = useState<Tab>("notes");
  const [version, setVersion] = useState(0);
  const recording = active?.id === id;
  const elapsed = useElapsed(recording ? active.started_at : null);

  const load = useCallback(async () => {
    const result = await commands.getMeeting(id);
    if (result.status === "ok") setDetail(result.data);
  }, [id]);

  useEffect(() => {
    void load();
    const unlisten = [
      listen<number>("meeting-changed", (e) => {
        if (e.payload === id) void load();
      }),
      listen<MeetingSegmentEvent>("meeting-segment", (e) => {
        if (e.payload.meeting_id !== id) return;
        setDetail((d) => {
          if (!d) return d;
          const hidden = new Set(e.payload.suppressed);
          const segments = d.segments
            .map((s) =>
              hidden.has(s.id) ? { ...s, suppressed_echo: true } : s,
            )
            .concat(e.payload.segment)
            .sort((a, b) => a.t0_ms - b.t0_ms);
          return { ...d, segments };
        });
      }),
    ];
    return () => unlisten.forEach((p) => void p.then((f) => f()));
  }, [id, load]);

  if (!detail) {
    return (
      <div className="flex justify-center py-10">
        <Spinner />
      </div>
    );
  }
  const { meeting } = detail;
  const notes = detail.ai_notes[Math.min(version, detail.ai_notes.length - 1)];
  const summarizing = meeting.status === "summarizing";

  const stop = async () => {
    const result = await commands.stopMeeting();
    if (result.status === "error") toast.error(result.error);
  };
  const regenerate = async () => {
    setVersion(0);
    const result = await commands.regenerateMeetingNotes(id);
    if (result.status === "error") toast.error(result.error);
  };
  const copy = async () => {
    const result = await commands.meetingMarkdown(id);
    if (result.status === "ok" && (await copyToClipboard(result.data))) {
      toast.success(t("meetings.copied"));
    }
  };
  const remove = async () => {
    const result = await commands.deleteMeeting(id);
    if (result.status === "error") toast.error(result.error);
    else onBack();
  };

  return (
    <div className="flex w-full flex-col gap-4">
      <button
        onClick={onBack}
        className="flex w-fit cursor-pointer items-center gap-1 text-sm text-ink-3 hover:text-text"
      >
        <ArrowLeft className="h-4 w-4" />
        {t("meetings.back")}
      </button>

      <div className="flex items-start justify-between gap-4">
        <div className="min-w-0">
          <input
            key={meeting.title}
            defaultValue={meeting.title}
            onBlur={(e) => {
              const title = e.target.value.trim();
              if (title && title !== meeting.title)
                void commands.renameMeeting(id, title);
            }}
            className="w-full truncate bg-transparent text-lg font-semibold focus:outline-none"
            aria-label={t("meetings.titleLabel")}
          />
          <p className="m-0 text-sm text-ink-3">
            {formatDate(meeting.started_at)}
            {meeting.ended_at &&
              ` · ${clock(meeting.ended_at - meeting.started_at)}`}
          </p>
        </div>
        {recording ? (
          <Button variant="danger" onClick={stop} className="shrink-0">
            {t("meetings.stop")}
          </Button>
        ) : (
          <div className="flex shrink-0 items-center gap-1">
            <Button
              variant="ghost"
              size="sm"
              onClick={copy}
              title={t("meetings.copy")}
            >
              <Copy className="h-4 w-4" />
            </Button>
            <Button
              variant="ghost"
              size="sm"
              onClick={regenerate}
              disabled={summarizing}
              title={t("meetings.regenerate")}
            >
              <RotateCcw className="h-4 w-4" />
            </Button>
            <Button
              variant="danger-ghost"
              size="sm"
              onClick={remove}
              title={t("meetings.delete")}
            >
              <Trash2 className="h-4 w-4" />
            </Button>
          </div>
        )}
      </div>

      {recording ? (
        <>
          <div className="flex items-center gap-2 text-sm">
            <span className="h-2 w-2 animate-pulse rounded-full bg-error" />
            <span className="tabular-nums">{clock(elapsed)}</span>
            <span className="text-ink-3">
              {active.has_system_audio
                ? t("meetings.recording.both")
                : t("meetings.recording.micOnly")}
            </span>
          </div>
          <Notepad meetingId={id} initial={detail.user_notes} />
          <div className="rounded-lg border border-line bg-background">
            <h3 className="m-0 border-b border-line px-4 py-2 text-xs font-medium uppercase tracking-wide text-ink-3">
              {t("meetings.tabs.transcript")}
            </h3>
            <Transcript segments={detail.segments} live />
          </div>
        </>
      ) : (
        <>
          <div className="flex gap-1 border-b border-line">
            {(["notes", "mine", "transcript"] as Tab[]).map((k) => (
              <button
                key={k}
                onClick={() => setTab(k)}
                className={`-mb-px cursor-pointer border-b-2 px-3 py-2 text-sm ${tab === k ? "border-logo-primary font-medium text-text" : "border-transparent text-ink-3 hover:text-text"}`}
              >
                {t(`meetings.tabs.${k}`)}
              </button>
            ))}
          </div>
          {tab === "notes" && (
            <div className="rounded-lg border border-line bg-background px-5 py-4">
              {summarizing ? (
                <div className="flex items-center gap-2 text-sm text-ink-3">
                  <Spinner />
                  {t("meetings.writing")}
                </div>
              ) : notes ? (
                <>
                  {detail.ai_notes.length > 1 && (
                    <select
                      value={version}
                      onChange={(e) => setVersion(Number(e.target.value))}
                      className="mb-3 rounded-md border border-line bg-background px-2 py-1 text-xs"
                    >
                      {detail.ai_notes.map((n, i) => (
                        <option key={n.id} value={i}>
                          {t("meetings.version", {
                            n: detail.ai_notes.length - i,
                            date: formatDate(n.created_at),
                          })}
                        </option>
                      ))}
                    </select>
                  )}
                  <div className="select-text text-sm">
                    <MarkdownContent markdown={notes.body_md} />
                  </div>
                </>
              ) : (
                <div className="flex flex-col items-start gap-2 text-sm">
                  <p className="m-0 text-ink-2">
                    {meeting.notes_error
                      ? t("meetings.notesFailed", {
                          error: meeting.notes_error,
                        })
                      : t("meetings.noNotes")}
                  </p>
                  <Button variant="secondary" size="sm" onClick={regenerate}>
                    {t("meetings.regenerate")}
                  </Button>
                </div>
              )}
            </div>
          )}
          {tab === "mine" && (
            <Notepad meetingId={id} initial={detail.user_notes} />
          )}
          {tab === "transcript" && (
            <div className="rounded-lg border border-line bg-background">
              <Transcript segments={detail.segments} />
            </div>
          )}
        </>
      )}
    </div>
  );
};

export const MeetingsSettings: React.FC = () => {
  const { t } = useTranslation();
  const { getSetting, updateSetting, isUpdating } = useSettings();
  const [meetings, setMeetings] = useState<MeetingSummary[]>([]);
  const [query, setQuery] = useState("");
  const [active, setActive] = useState<ActiveMeeting | null>(null);
  const [open, setOpen] = useState<number | null>(null);
  const [starting, setStarting] = useState(false);
  const elapsed = useElapsed(active?.started_at ?? null);

  const refresh = useCallback(async () => {
    const [list, current] = await Promise.all([
      commands.listMeetings(query || null),
      commands.getActiveMeeting(),
    ]);
    if (list.status === "ok") setMeetings(list.data);
    setActive(current);
  }, [query]);

  useEffect(() => {
    void refresh();
    const unlisten = listen("meeting-changed", () => void refresh());
    return () => void unlisten.then((f) => f());
  }, [refresh]);

  const start = async () => {
    setStarting(true);
    const result = await commands.startMeeting();
    setStarting(false);
    if (result.status === "ok") {
      setActive(result.data);
      setOpen(result.data.id);
    } else {
      toast.error(t("meetings.startFailed", { error: result.error }));
    }
  };

  const statusLabel = useMemo(
    () => (m: MeetingSummary) =>
      active?.id === m.id
        ? t("meetings.status.recording")
        : m.status === "summarizing"
          ? t("meetings.status.summarizing")
          : null,
    [active, t],
  );

  if (open !== null) {
    return (
      <div className="w-full max-w-3xl">
        <MeetingView id={open} active={active} onBack={() => setOpen(null)} />
      </div>
    );
  }

  return (
    <div className="flex w-full max-w-3xl flex-col gap-4">
      {active ? (
        <button
          onClick={() => setOpen(active.id)}
          className="flex cursor-pointer items-center gap-3 rounded-lg border border-error/40 bg-error-wash px-4 py-3 text-start text-sm"
        >
          <span className="h-2 w-2 animate-pulse rounded-full bg-error" />
          <span className="font-medium">{t("meetings.recording.banner")}</span>
          <span className="tabular-nums text-ink-2">{clock(elapsed)}</span>
          <span className="ms-auto text-ink-3">{t("meetings.open")}</span>
        </button>
      ) : (
        <div className="flex items-center justify-between gap-4 rounded-lg border border-line bg-background px-4 py-3">
          <p className="m-0 text-sm text-ink-2">{t("meetings.intro")}</p>
          <Button
            onClick={start}
            disabled={starting}
            className="flex shrink-0 items-center gap-1.5"
          >
            <Mic className="h-4 w-4" />
            {t("meetings.new")}
          </Button>
        </div>
      )}

      <div className="relative">
        <Search className="pointer-events-none absolute start-3 top-1/2 h-4 w-4 -translate-y-1/2 text-ink-3" />
        <Input
          type="search"
          value={query}
          onChange={(e) => setQuery(e.target.value)}
          placeholder={t("meetings.search")}
          className="w-full ps-9"
        />
      </div>

      <div className="divide-y divide-line overflow-hidden rounded-lg border border-line bg-background">
        {meetings.length === 0 ? (
          <p className="m-0 px-4 py-6 text-center text-sm text-ink-3">
            {query ? t("meetings.noResults") : t("meetings.empty")}
          </p>
        ) : (
          meetings.map((m) => (
            <button
              key={m.id}
              onClick={() => setOpen(m.id)}
              className="flex w-full cursor-pointer items-center justify-between gap-4 px-4 py-3 text-start hover:bg-wash"
            >
              <span className="min-w-0">
                <span className="block truncate text-sm font-medium">
                  {m.title}
                </span>
                <span className="block text-xs text-ink-3">
                  {formatDate(m.started_at)}
                  {m.ended_at && ` · ${clock(m.ended_at - m.started_at)}`}
                </span>
              </span>
              {statusLabel(m) && (
                <span className="shrink-0 text-xs text-ink-3">
                  {statusLabel(m)}
                </span>
              )}
            </button>
          ))
        )}
      </div>
      <SettingsGroup>
        <ToggleSwitch
          checked={getSetting("meeting_detection") ?? true}
          onChange={(on) => updateSetting("meeting_detection", on)}
          isUpdating={isUpdating("meeting_detection")}
          label={t("meetings.detect.title")}
          description={t("meetings.detect.description")}
          descriptionMode="inline"
          grouped
        />
      </SettingsGroup>
      <p className="m-0 px-1 text-xs text-ink-3">{t("meetings.consent")}</p>
    </div>
  );
};
