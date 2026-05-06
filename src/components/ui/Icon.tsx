import {
  Microphone,
  Waves,
  FileText,
  Sparkle,
  Equalizer,
  FloppyDisk,
  Lock,
  WarningCircle,
  Play,
  Pause,
  Stop,
  Trash,
  Plus,
  X,
  Check,
  Pencil,
  CaretRight,
  Folder,
  Waveform,
  Gear,
  Question,
  Info,
  SidebarSimple,
  type IconProps as PhosphorIconProps,
  type IconWeight,
} from "@phosphor-icons/react";

export type IconName =
  | "mic" | "waveform" | "doc-text" | "sparkles" | "equalizer" | "save"
  | "lock" | "warning"
  | "play" | "pause" | "stop" | "trash"
  | "plus" | "x" | "check" | "pencil" | "chevron-right"
  | "folder" | "recording"
  | "gear" | "question" | "info"
  | "sidebar";

export type IconSize = "sm" | "md" | "lg";

const REGISTRY: Record<IconName, React.ComponentType<PhosphorIconProps>> = {
  mic: Microphone,
  waveform: Waves,
  "doc-text": FileText,
  sparkles: Sparkle,
  equalizer: Equalizer,
  save: FloppyDisk,
  lock: Lock,
  warning: WarningCircle,
  play: Play,
  pause: Pause,
  stop: Stop,
  trash: Trash,
  plus: Plus,
  x: X,
  check: Check,
  pencil: Pencil,
  "chevron-right": CaretRight,
  folder: Folder,
  recording: Waveform,
  gear: Gear,
  question: Question,
  info: Info,
  sidebar: SidebarSimple,
};

const SIZE_PX: Record<IconSize, number> = { sm: 14, md: 20, lg: 32 };
const SIZE_WEIGHT: Record<IconSize, IconWeight> = {
  sm: "bold",
  md: "regular",
  lg: "light",
};

interface Props {
  name: IconName;
  size?: IconSize;
  className?: string;
  "aria-label"?: string;
}

export function Icon({
  name,
  size = "md",
  className,
  "aria-label": ariaLabel,
}: Props) {
  const Component = REGISTRY[name];
  const px = SIZE_PX[size];
  const weight = SIZE_WEIGHT[size];

  if (ariaLabel) {
    return (
      <Component
        size={px}
        weight={weight}
        className={className}
        aria-label={ariaLabel}
      />
    );
  }
  return (
    <Component
      size={px}
      weight={weight}
      className={className}
      aria-hidden
    />
  );
}
