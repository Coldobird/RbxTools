import type { ReactNode } from "react";
import steamPixelLogo from "../assets/steam28.png";
import "./pixel-spinner.css";

type IconProps = { size?: number; className?: string };
type IconName =
  | "check" | "arrowLeft" | "alert" | "power" | "chevronDown" | "chevronRight"
  | "folder" | "grid" | "activity" | "preview" | "restart" | "settings"
  | "wifi" | "wifiOff" | "close";

// Keep edges square so the 16px RBX drawings and 24px sourced glyphs match the
// app badge and landscape when scaled by the desktop webview.
const glyphs: Record<IconName, ReactNode> = {
  check: <path d="M2 8h2v2h2v2h2v-2h2V8h2V6h2v2h-2v2h-2v2H8v2H6v-2H4v-2H2z" />,
  arrowLeft: <path d="M6 2h2v2H6v2H4v2h10v2H4v2h2v2h2v2H6v-2H4v-2H2V6h2V4h2z" />,
  alert: <><path d="M5 1h6v2h2v2h2v6h-2v2h-2v2H5v-2H3v-2H1V5h2V3h2zm1 2v1H4v2H3v4h1v2h2v1h4v-1h2v-2h1V6h-1V4h-2V3z" /><path d="M7 5h2v5H7zm0 6h2v2H7z" /></>,
  power: <><path d="M7 1h2v8H7z" /><path d="M4 3h2v2H4v1H3v5h1v1h2v1h4v-1h2v-1h1V6h-1V5h-2V3h2v1h2v2h1v5h-1v2h-2v1H4v-1H2v-2H1V6h1V4h2z" /></>,
  chevronDown: <path d="M1 5h2v2h2v2h2v2h2V9h2V7h2V5h2v2h-2v2h-2v2H9v2H7v-2H5V9H3V7H1z" />,
  chevronRight: <path d="M5 1h2v2h2v2h2v2h2v2h-2v2H9v2H7v2H5v-2h2v-2h2V9h2V7H9V5H7V3H5z" />,
  folder: <path d="M1 3h5v2h8v2H3v6h10V8h2v6H1zm2 2v1h9V5H6V3H3z" />,
  grid: <path d="M1 1h6v6H1zm8 0h6v6H9zM1 9h6v6H1zm8 0h6v6H9z" />,
  activity: <><path d="M2 1h12v14H2zm2 2v10h8V3z" /><path d="M5 5h2v2H5zm3 0h3v2H8zM5 8h2v2H5zm3 0h3v2H8zM5 11h6v1H5z" /></>,
  preview: <><path d="M2 1h12v14H2zm2 2v10h8V3z" /><path d="M5 5h2v2H5zm3 0h3v2H8zM5 8h2v2H5zm3 0h3v2H8zM5 11h6v1H5z" /></>,
  restart: <path d="M6 1h5v2H6V2H4v2H3v2H2v4h1v2h2v1h5v-1h2v-2h2v3h-2v2H5v-1H3v-2H1v-2H0V6h1V4h2V2h3zm6 1h2v6H8V6h4V2z" />,
  settings: <><path d="M6 0h4v2h2v2h2v2h2v4h-2v2h-2v2h-2v2H6v-2H4v-2H2v-2H0V6h2V4h2V2h2zm1 3H5v2H3v2H2v2h1v2h2v2h2v1h2v-1h2v-2h2V9h1V7h-1V5h-2V3H9z" /><path d="M6 6h4v4H6z" /></>,
  wifi: <path d="M4 3h8v1h2v2h-2V5H4v1H2V4h2zM5 7h6v1h2v2h-2V9H5v1H3V8h2zm2 4h2v1h1v1H9v2H7v-2H6v-1h1z" />,
  wifiOff: <><path d="M4 3h8v1h2v2h-2V5H5zm1 4h6v1h2v2h-2V9H7zm2 4h2v1h1v1H9v2H7v-2H6v-1h1z" /><path d="M1 1h2v2h2v2h2v2h2v2h2v2h2v2h2v2h-2v-2h-2v-2H9V9H7V7H5V5H3V3H1z" /></>,
  close: <path d="M2 1h2v2h2v2h4V3h2V1h2v2h-2v2h-2v2H9v2h1v2h2v2h2v2h-2v-2h-2v-2H6v2H4v2H2v-2h2v-2h2V9h1V7H6V5H4V3H2z" />,
};

// Selected from Pixelarticons 2.4.1 (MIT, Gerrit Halfmann). Wi-Fi off is a
// paired RBX variant of that set's Wi-Fi glyph; its stepped slash stays legible
// at the small status-icon size.
const sourcedGlyphs: Partial<Record<IconName, ReactNode>> = {
  check: <path d="M10 18H8v-2h2v2Zm-2-2H6v-2h2v2Zm4-2v2h-2v-2h2Zm-6 0H4v-2h2v2Zm8 0h-2v-2h2v2Zm2-2h-2v-2h2v2Zm2-2h-2V8h2v2Zm2-2h-2V6h2v2Z" />,
  restart: <path d="M13 20H9V18H13V20ZM19 16H21V18H19V20H17V18H15V16H17V8H19V16ZM9 18H7V16H9V18ZM7 6H9V8H7V16H5V8H3V6H5V4H7V6ZM15 16H13V14H15V16ZM23 16H21V14H23V16ZM3 10H1V8H3V10ZM11 10H9V8H11V10ZM17 8H15V6H17V8ZM15 6H11V4H15V6Z" />,
  grid: <path d="M3 4h2v17H3zm4 4h2v13H7zm4-2h2v15h-2zm4 0h2v5h-2zm2 5h2v5h-2zm2 5h2v5h-2z" />,
  settings: <path d="M8 14H7v6H5v-6H2v-2h6v2Zm5 6h-2V10h2v10Zm9-2h-3v2h-2v-2h-1v-2h6v2Zm-3-4h-2V4h2v10ZM7 10H5V4h2v6Zm6-4h2v2H9V6h2V4h2v2Z" />,
  activity: <path d="M4 2h16v2H4zm2 5h2v2H6zm4 0h8v2h-8zm-4 4h2v2H6zm4 0h8v2h-8zm-4 4h2v2H6zm4 0h8v2h-8zm-6 5h16v2H4zM2 4h2v16H2zm18 0h2v16h-2z" />,
  power: <path d="M20 20H4V4H20V20ZM6 18H18V6H6V18ZM14 14H10V10H14V14Z" />,
  folder: <path d="M2 4h10v2H2zm0 14h20v2H2zM20 6h2v12h-2zM2 6h2v12H2zm8 0h10v2H10z" />,
  wifi: <path d="M11 19h2v2h-2zm-4-3h2v2H7zm8 0h2v2h-2zm-6-2h6v2H9zm-5-1h2v2H4zm2-2h2v2H6zm2-2h8v2H8zm-7 1h2v2H1zm20 0h2v2h-2zM3 8h2v2H3zm2-2h2v2H5zm2-2h10v2H7zm12 4h2v2h-2zm-2-2h2v2h-2zm1 7h2v2h-2zm-2-2h2v2h-2z" />,
  arrowLeft: <path d="M8 13v-2h2v2H8Zm2-2V9h2v2h-2Zm0 4v-2h2v2h-2Zm2-6V7h2v2h-2Zm0 8v-2h2v2h-2Zm2-10V5h2v2h-2Zm0 12v-2h2v2h-2Z" />,
  chevronRight: <path d="M16 13v-2h-2v2h2Zm-2-2V9h-2v2h2Zm0 4v-2h-2v2h2Zm-2-6V7h-2v2h2Zm0 8v-2h-2v2h2ZM10 7V5H8v2h2Zm0 12v-2H8v2h2Z" />,
  chevronDown: <path d="M13 16h-2v-2h2v2Zm-2-2H9v-2h2v2Zm4 0h-2v-2h2v2Zm-6-2H7v-2h2v2Zm8 0h-2v-2h2v2ZM7 10H5V8h2v2Zm12 0h-2V8h2v2Z" />,
  alert: <path d="M4 2h16v2H4zm0 18h16v2H4zM20 4h2v16h-2zM2 4h2v16H2zm9 2h2v8h-2zm0 10h2v2h-2z" />,
  preview: <path d="M4 2h16v2H4zm0 18h16v2H4zM2 4h2v16H2zm18 0h2v16h-2zm-9 5h2V7h-2zm0 8h2v-6h-2z" />,
  close: <path d="M7 19H5V17H7V19ZM19 19H17V17H19V19ZM9 15V17H7V15H9ZM17 17H15V15H17V17ZM11 15H9V13H11V15ZM15 15H13V13H15V15ZM13 13H11V11H13V13ZM11 11H9V9H11V11ZM15 11H13V9H15V11ZM9 9H7V7H9V9ZM17 9H15V7H17V9ZM7 7H5V5H7V7ZM19 7H17V5H19V7Z" />,
  wifiOff: <><path d="M11 19h2v2h-2zm-4-3h2v2H7zm8 0h2v2h-2zm-6-2h6v2H9zm-5-1h2v2H4zm2-2h2v2H6zm2-2h8v2H8zm-7 1h2v2H1zm20 0h2v2h-2zM3 8h2v2H3zm2-2h2v2H5zm2-2h10v2H7zm12 4h2v2h-2zm-2-2h2v2h-2zm1 7h2v2h-2zm-2-2h2v2h-2z" /><path d="M2 2h2v2H2zm2 2h2v2H4zm2 2h2v2H6zm2 2h2v2H8zm2 2h2v2h-2zm2 2h2v2h-2zm2 2h2v2h-2zm2 2h2v2h-2zm2 2h2v2h-2zm2 2h2v2h-2z" /></>,
};

function makeIcon(name: IconName) {
  function PixelIcon({ size = 16, className }: IconProps) {
    const sourced = sourcedGlyphs[name];
    return <svg className={`pixel-icon${className ? ` ${className}` : ""}`} width={size} height={size} viewBox={sourced ? "0 0 24 24" : "0 0 16 16"} fill="currentColor" shapeRendering="crispEdges" aria-hidden="true">{sourced ?? glyphs[name]}</svg>;
  }
  PixelIcon.displayName = `Pixel${name[0].toUpperCase()}${name.slice(1)}`;
  return PixelIcon;
}

export const Check = makeIcon("check");
export const ArrowLeft = makeIcon("arrowLeft");
export const CircleAlert = makeIcon("alert");
export const Power = makeIcon("power");
export const ChevronDown = makeIcon("chevronDown");
export const ChevronRight = makeIcon("chevronRight");
export const FolderOpen = makeIcon("folder");
export const Grid2X2 = makeIcon("grid");
// The four-block chase adapts SVG Spinners' blocks-scale pattern (MIT, Utkarsh
// Verma). Each square pulses in turn for stepped GBA-style motion.
export function LoaderCircle({ size = 16, className }: IconProps) {
  return <svg className={`pixel-icon pixel-spinner${className ? ` ${className}` : ""}`} width={size} height={size} viewBox="0 0 24 24" fill="currentColor" shapeRendering="crispEdges" aria-hidden="true">
    {[[2, 2], [15, 2], [15, 15], [2, 15]].map(([x, y], index) => <rect className="pixel-spinner-block" key={index} x={x} y={y} width="7" height="7" style={{ animationDelay: `${-index * .3}s` }} />)}
  </svg>;
}
export const Radio = makeIcon("activity");
export const Preview = makeIcon("preview");
export const RotateCw = makeIcon("restart");
export const Settings2 = makeIcon("settings");
export const Wifi = makeIcon("wifi");
export const WifiOff = makeIcon("wifiOff");
export const X = makeIcon("close");

export function SteamFriendLogo({ small = false }: { small?: boolean }) {
  return <img className={small ? "steam-friend-logo small" : "steam-friend-logo"} src={steamPixelLogo} alt="" aria-hidden="true" />;
}
