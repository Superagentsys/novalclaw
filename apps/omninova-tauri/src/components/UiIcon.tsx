import type { ComponentType, CSSProperties } from "react";
import ApiOutlined from "@ant-design/icons/ApiOutlined";
import AppstoreOutlined from "@ant-design/icons/AppstoreOutlined";
import ApartmentOutlined from "@ant-design/icons/ApartmentOutlined";
import AudioOutlined from "@ant-design/icons/AudioOutlined";
import BarChartOutlined from "@ant-design/icons/BarChartOutlined";
import BgColorsOutlined from "@ant-design/icons/BgColorsOutlined";
import BookOutlined from "@ant-design/icons/BookOutlined";
import BulbOutlined from "@ant-design/icons/BulbOutlined";
import CheckOutlined from "@ant-design/icons/CheckOutlined";
import CloseOutlined from "@ant-design/icons/CloseOutlined";
import CloudOutlined from "@ant-design/icons/CloudOutlined";
import CodeOutlined from "@ant-design/icons/CodeOutlined";
import DatabaseOutlined from "@ant-design/icons/DatabaseOutlined";
import DeleteOutlined from "@ant-design/icons/DeleteOutlined";
import DesktopOutlined from "@ant-design/icons/DesktopOutlined";
import DollarOutlined from "@ant-design/icons/DollarOutlined";
import EditOutlined from "@ant-design/icons/EditOutlined";
import ExperimentOutlined from "@ant-design/icons/ExperimentOutlined";
import FileImageOutlined from "@ant-design/icons/FileImageOutlined";
import FileOutlined from "@ant-design/icons/FileOutlined";
import FileTextOutlined from "@ant-design/icons/FileTextOutlined";
import FolderOpenOutlined from "@ant-design/icons/FolderOpenOutlined";
import GlobalOutlined from "@ant-design/icons/GlobalOutlined";
import HistoryOutlined from "@ant-design/icons/HistoryOutlined";
import MenuFoldOutlined from "@ant-design/icons/MenuFoldOutlined";
import MenuUnfoldOutlined from "@ant-design/icons/MenuUnfoldOutlined";
import MessageOutlined from "@ant-design/icons/MessageOutlined";
import MobileOutlined from "@ant-design/icons/MobileOutlined";
import MoonOutlined from "@ant-design/icons/MoonOutlined";
import PaperClipOutlined from "@ant-design/icons/PaperClipOutlined";
import PlusOutlined from "@ant-design/icons/PlusOutlined";
import ReloadOutlined from "@ant-design/icons/ReloadOutlined";
import RobotOutlined from "@ant-design/icons/RobotOutlined";
import SafetyCertificateOutlined from "@ant-design/icons/SafetyCertificateOutlined";
import SearchOutlined from "@ant-design/icons/SearchOutlined";
import SendOutlined from "@ant-design/icons/SendOutlined";
import SettingOutlined from "@ant-design/icons/SettingOutlined";
import SkinOutlined from "@ant-design/icons/SkinOutlined";
import SoundOutlined from "@ant-design/icons/SoundOutlined";
import SunOutlined from "@ant-design/icons/SunOutlined";
import SyncOutlined from "@ant-design/icons/SyncOutlined";
import TeamOutlined from "@ant-design/icons/TeamOutlined";
import ToolOutlined from "@ant-design/icons/ToolOutlined";
import VideoCameraOutlined from "@ant-design/icons/VideoCameraOutlined";
import WarningOutlined from "@ant-design/icons/WarningOutlined";

type AntIconComponent = ComponentType<{
  className?: string;
  style?: CSSProperties;
  "aria-hidden"?: boolean;
}>;

const ICONS = {
  api: ApiOutlined,
  apps: AppstoreOutlined,
  agent: RobotOutlined,
  audio: AudioOutlined,
  chart: BarChartOutlined,
  check: CheckOutlined,
  close: CloseOutlined,
  cloud: CloudOutlined,
  code: CodeOutlined,
  connections: ApartmentOutlined,
  database: DatabaseOutlined,
  delete: DeleteOutlined,
  desktop: DesktopOutlined,
  edit: EditOutlined,
  experiment: ExperimentOutlined,
  file: FileOutlined,
  fileImage: FileImageOutlined,
  fileText: FileTextOutlined,
  folder: FolderOpenOutlined,
  global: GlobalOutlined,
  history: HistoryOutlined,
  menuFold: MenuFoldOutlined,
  menuUnfold: MenuUnfoldOutlined,
  message: MessageOutlined,
  mobile: MobileOutlined,
  moon: MoonOutlined,
  paperclip: PaperClipOutlined,
  palette: BgColorsOutlined,
  plus: PlusOutlined,
  reload: ReloadOutlined,
  safety: SafetyCertificateOutlined,
  search: SearchOutlined,
  send: SendOutlined,
  settings: SettingOutlined,
  skin: SkinOutlined,
  sound: SoundOutlined,
  sun: SunOutlined,
  sync: SyncOutlined,
  team: TeamOutlined,
  tool: ToolOutlined,
  video: VideoCameraOutlined,
  wallet: DollarOutlined,
  warning: WarningOutlined,
  writing: EditOutlined,
  knowledge: BookOutlined,
  idea: BulbOutlined,
} satisfies Record<string, AntIconComponent>;

export type UiIconName = keyof typeof ICONS;

interface UiIconProps {
  name: UiIconName;
  className?: string;
  size?: number;
}

/**
 * Product icon wrapper. All glyphs come from Ant Design Icons (MIT), which
 * keeps visual weight consistent and avoids platform-dependent Emoji rendering.
 */
export function UiIcon({ name, className, size = 18 }: UiIconProps) {
  const Icon = ICONS[name];
  return (
    <Icon
      className={className}
      style={{ fontSize: size }}
      aria-hidden
    />
  );
}
