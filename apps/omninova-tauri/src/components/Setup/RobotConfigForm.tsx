import { type RobotConfig } from "../../types/config";

type Props = {
  value: RobotConfig;
  onChange: (next: RobotConfig) => void;
};

const toNumber = (value: string) => {
  const parsed = Number(value);
  return Number.isNaN(parsed) ? 0 : parsed;
};

const parseNumberList = (value: string) =>
  value
    .split(",")
    .map((item) => item.trim())
    .filter(Boolean)
    .map((item) => Number(item))
    .filter((item) => !Number.isNaN(item));

const parseOptionalPair = (value: string): [number, number] | undefined => {
  const parsed = parseNumberList(value);
  if (parsed.length !== 2) {
    return undefined;
  }
  return [parsed[0], parsed[1]];
};

export function RobotConfigForm({ value, onChange }: Props) {
  const updateDrive = (key: keyof RobotConfig["drive"], nextValue: string) => {
    onChange({
      ...value,
      drive: {
        ...value.drive,
        [key]: ["max_speed", "max_rotation"].includes(key)
          ? toNumber(nextValue)
          : nextValue || undefined,
      },
    });
  };

  const updateCamera = (key: keyof RobotConfig["camera"], nextValue: string) => {
    onChange({
      ...value,
      camera: {
        ...value.camera,
        [key]: ["width", "height"].includes(key)
          ? toNumber(nextValue)
          : nextValue,
      },
    });
  };

  const updateAudio = (key: keyof RobotConfig["audio"], nextValue: string) => {
    onChange({
      ...value,
      audio: {
        ...value.audio,
        [key]: nextValue || undefined,
      },
    });
  };

  const updateSensors = (
    key: keyof RobotConfig["sensors"],
    nextValue: string
  ) => {
    if (key === "motion_pins") {
      onChange({
        ...value,
        sensors: {
          ...value.sensors,
          motion_pins: parseNumberList(nextValue),
        },
      });
      return;
    }
    if (key === "ultrasonic_pins") {
      onChange({
        ...value,
        sensors: {
          ...value.sensors,
          ultrasonic_pins: parseOptionalPair(nextValue),
        },
      });
      return;
    }
    onChange({
      ...value,
      sensors: {
        ...value.sensors,
        [key]: nextValue || undefined,
      },
    });
  };

  const updateSafety = (
    key: keyof RobotConfig["safety"],
    nextValue: string
  ) => {
    if (["min_obstacle_distance", "slow_zone_multiplier", "approach_speed_limit"].includes(key)) {
      onChange({
        ...value,
        safety: {
          ...value.safety,
          [key]: toNumber(nextValue),
        },
      });
      return;
    }
    if (key === "estop_pin") {
      onChange({
        ...value,
        safety: {
          ...value.safety,
          estop_pin: nextValue ? toNumber(nextValue) : undefined,
        },
      });
      return;
    }
    if (key === "bump_sensor_pins") {
      onChange({
        ...value,
        safety: {
          ...value.safety,
          bump_sensor_pins: parseNumberList(nextValue),
        },
      });
    }
  };

  return (
    <div className="setup-section">
      <h2>机器人配置</h2>
      <div className="setup-group">
        <h3>驱动</h3>
        <div className="setup-grid">
          <label>
            后端
            <input
              value={value.drive.backend}
              onChange={(event) => updateDrive("backend", event.target.value)}
            />
          </label>
          <label>
            ROS2 话题
            <input
              value={value.drive.ros2_topic ?? ""}
              onChange={(event) => updateDrive("ros2_topic", event.target.value)}
            />
          </label>
          <label>
            串口
            <input
              value={value.drive.serial_port ?? ""}
              onChange={(event) => updateDrive("serial_port", event.target.value)}
            />
          </label>
          <label>
            最大速度
            <input
              type="number"
              step="0.1"
              value={value.drive.max_speed}
              onChange={(event) => updateDrive("max_speed", event.target.value)}
            />
          </label>
          <label>
            最大旋转
            <input
              type="number"
              step="0.1"
              value={value.drive.max_rotation}
              onChange={(event) => updateDrive("max_rotation", event.target.value)}
            />
          </label>
        </div>
      </div>
      <div className="setup-group">
        <h3>相机</h3>
        <div className="setup-grid">
          <label>
            设备
            <input
              value={value.camera.device}
              onChange={(event) => updateCamera("device", event.target.value)}
            />
          </label>
          <label>
            宽度
            <input
              type="number"
              value={value.camera.width}
              onChange={(event) => updateCamera("width", event.target.value)}
            />
          </label>
          <label>
            高度
            <input
              type="number"
              value={value.camera.height}
              onChange={(event) => updateCamera("height", event.target.value)}
            />
          </label>
          <label>
            视觉模型
            <input
              value={value.camera.vision_model}
              onChange={(event) => updateCamera("vision_model", event.target.value)}
            />
          </label>
          <label>
            Ollama 地址
            <input
              value={value.camera.ollama_url}
              onChange={(event) => updateCamera("ollama_url", event.target.value)}
            />
          </label>
        </div>
      </div>
      <div className="setup-group">
        <h3>音频</h3>
        <div className="setup-grid">
          <label>
            麦克风
            <input
              value={value.audio.mic_device}
              onChange={(event) => updateAudio("mic_device", event.target.value)}
            />
          </label>
          <label>
            扬声器
            <input
              value={value.audio.speaker_device}
              onChange={(event) => updateAudio("speaker_device", event.target.value)}
            />
          </label>
          <label>
            Whisper 模型
            <input
              value={value.audio.whisper_model}
              onChange={(event) => updateAudio("whisper_model", event.target.value)}
            />
          </label>
          <label>
            Whisper 路径
            <input
              value={value.audio.whisper_path ?? ""}
              onChange={(event) => updateAudio("whisper_path", event.target.value)}
            />
          </label>
          <label>
            Piper 路径
            <input
              value={value.audio.piper_path ?? ""}
              onChange={(event) => updateAudio("piper_path", event.target.value)}
            />
          </label>
          <label>
            Piper 声音
            <input
              value={value.audio.piper_voice ?? ""}
              onChange={(event) => updateAudio("piper_voice", event.target.value)}
            />
          </label>
        </div>
      </div>
      <div className="setup-group">
        <h3>传感器</h3>
        <div className="setup-grid">
          <label>
            雷达端口
            <input
              value={value.sensors.lidar_port ?? ""}
              onChange={(event) => updateSensors("lidar_port", event.target.value)}
            />
          </label>
          <label>
            雷达类型
            <input
              value={value.sensors.lidar_type}
              onChange={(event) => updateSensors("lidar_type", event.target.value)}
            />
          </label>
          <label>
            Motion Pins
            <input
              value={value.sensors.motion_pins.join(",")}
              onChange={(event) => updateSensors("motion_pins", event.target.value)}
              placeholder="17,27"
            />
          </label>
          <label>
            Ultrasonic Pins
            <input
              value={value.sensors.ultrasonic_pins?.join(",") ?? ""}
              onChange={(event) => updateSensors("ultrasonic_pins", event.target.value)}
              placeholder="23,24"
            />
          </label>
        </div>
      </div>
      <div className="setup-group">
        <h3>安全</h3>
        <div className="setup-grid">
          <label>
            最小障碍距离
            <input
              type="number"
              step="0.1"
              value={value.safety.min_obstacle_distance}
              onChange={(event) =>
                updateSafety("min_obstacle_distance", event.target.value)
              }
            />
          </label>
          <label>
            减速倍率
            <input
              type="number"
              step="0.1"
              value={value.safety.slow_zone_multiplier}
              onChange={(event) =>
                updateSafety("slow_zone_multiplier", event.target.value)
              }
            />
          </label>
          <label>
            接近速度上限
            <input
              type="number"
              step="0.1"
              value={value.safety.approach_speed_limit}
              onChange={(event) =>
                updateSafety("approach_speed_limit", event.target.value)
              }
            />
          </label>
          <label>
            急停引脚
            <input
              type="number"
              value={value.safety.estop_pin ?? ""}
              onChange={(event) => updateSafety("estop_pin", event.target.value)}
            />
          </label>
          <label>
            碰撞引脚
            <input
              value={value.safety.bump_sensor_pins.join(",")}
              onChange={(event) =>
                updateSafety("bump_sensor_pins", event.target.value)
              }
              placeholder="5,6"
            />
          </label>
        </div>
      </div>
    </div>
  );
}
