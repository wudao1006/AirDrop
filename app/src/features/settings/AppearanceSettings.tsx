import { Toggle } from "../../components/Toggle";
import type { AppSettings, PlatformKind } from "../../model";
import { DEFAULT_APPEARANCE_SETTINGS, type AppearanceSettings as AppearanceSettingsValue } from "./appearance";

const ACCENT_PRESETS = ["#168fae", "#5b7cfa", "#8b5cf6", "#e05d8b", "#e48632"] as const;

const colorInputValue = (color: string): string =>
  /^#[\da-f]{3}$/i.test(color)
    ? `#${[...color.slice(1)].map((digit) => digit.repeat(2)).join("")}`
    : color;

type AppearanceSettingsProps = {
  settings: AppSettings;
  platform: PlatformKind;
  onUpdate: (settings: Partial<AppSettings>) => void;
};

type RangeSettingProps = {
  id: string;
  label: string;
  description: string;
  value: number;
  minimum: number;
  maximum: number;
  step: number;
  displayValue: string;
  onChange: (value: number) => void;
};

function RangeSetting({ id, label, description, value, minimum, maximum, step, displayValue, onChange }: RangeSettingProps) {
  return <div className="appearance-range-row">
    <div className="appearance-range-heading">
      <label htmlFor={id}><strong>{label}</strong><span>{description}</span></label>
      <output htmlFor={id}>{displayValue}</output>
    </div>
    <input
      id={id}
      className="appearance-range"
      aria-label={label}
      type="range"
      min={minimum}
      max={maximum}
      step={step}
      value={value}
      onChange={(event) => onChange(event.currentTarget.valueAsNumber)}
    />
  </div>;
}

export function AppearanceSettings({ settings, platform, onUpdate }: AppearanceSettingsProps) {
  const updateAppearance = <Key extends keyof AppearanceSettingsValue>(key: Key, value: AppearanceSettingsValue[Key]) => {
    onUpdate({ [key]: value });
  };

  return <section className="page-section appearance-section" aria-labelledby="appearance-settings-title">
    <div className="section-title appearance-section-title">
      <div>
        <h2 id="appearance-settings-title">外观与液态玻璃</h2>
        <p>调整主题色、透明材质和窗口轮廓，改动会即时应用。</p>
      </div>
      <button type="button" className="button appearance-reset" onClick={() => onUpdate({ ...DEFAULT_APPEARANCE_SETTINGS })}>恢复默认外观</button>
    </div>
    <div className="card appearance-card">
      <div className="appearance-basic-grid">
        <div className="appearance-control-block">
          <label className="appearance-control-label" htmlFor="appearance-theme"><strong>主题</strong><span>跟随系统或使用固定明暗主题</span></label>
          <select id="appearance-theme" aria-label="主题" className="select appearance-select" value={settings.theme} onChange={(event) => updateAppearance("theme", event.currentTarget.value as AppSettings["theme"])}>
            <option value="system">跟随系统</option>
            <option value="light">亮色</option>
            <option value="dark">暗色</option>
          </select>
        </div>
        <fieldset className="appearance-control-block appearance-accent-fieldset">
          <legend className="appearance-control-label"><strong>强调色</strong><span>选择预设或使用自定义颜色</span></legend>
          <div className="appearance-color-controls">
            <div className="appearance-swatches" aria-label="强调色预设">
              {ACCENT_PRESETS.map((color, index) => <button
                key={color}
                type="button"
                className="appearance-swatch"
                style={{ backgroundColor: color }}
                aria-label={`强调色预设 ${index + 1}`}
                aria-pressed={settings.accentColor.toLowerCase() === color}
                onClick={() => updateAppearance("accentColor", color)}
              />)}
            </div>
            <label className="appearance-color-picker" title="自定义强调色">
              <span className="sr-only">自定义强调色</span>
              <input type="color" aria-label="自定义强调色" value={colorInputValue(settings.accentColor)} onChange={(event) => updateAppearance("accentColor", event.currentTarget.value)} />
              <span aria-hidden="true">+</span>
            </label>
          </div>
        </fieldset>
      </div>
      <div className="appearance-ranges">
        <RangeSetting id="appearance-opacity" label="窗口不透明度" description="控制桌面窗口整体透明程度" value={settings.windowOpacity} minimum={0.72} maximum={1} step={0.01} displayValue={`${Math.round(settings.windowOpacity * 100)}%`} onChange={(value) => updateAppearance("windowOpacity", value)} />
        <RangeSetting id="appearance-blur" label="玻璃模糊" description="控制背景透过材质时的柔化范围" value={settings.blurStrength} minimum={12} maximum={56} step={1} displayValue={`${settings.blurStrength}px`} onChange={(value) => updateAppearance("blurStrength", value)} />
        <RangeSetting id="appearance-saturation" label="玻璃饱和度" description="增强或减弱玻璃后方的色彩" value={settings.glassSaturation} minimum={0.9} maximum={1.6} step={0.05} displayValue={`${Math.round(settings.glassSaturation * 100)}%`} onChange={(value) => updateAppearance("glassSaturation", value)} />
        <RangeSetting id="appearance-radius" label="窗口圆角" description="调整窗口与内容面板的轮廓弧度" value={settings.cornerRadius} minimum={14} maximum={30} step={1} displayValue={`${settings.cornerRadius}px`} onChange={(value) => updateAppearance("cornerRadius", value)} />
        <RangeSetting id="appearance-highlight" label="高光强度" description="调整液态玻璃表面的反射纹理" value={settings.highlightStrength} minimum={0} maximum={0.6} step={0.02} displayValue={`${Math.round(settings.highlightStrength * 100)}%`} onChange={(value) => updateAppearance("highlightStrength", value)} />
      </div>
      {platform === "desktop" && <div className="appearance-desktop-option">
        <Toggle label="桌面悬浮球" description="在桌面边缘显示快速入口；默认关闭，启用后可更快打开剪贴板切换器。" checked={settings.floatingOrbEnabled} onChange={(value) => updateAppearance("floatingOrbEnabled", value)} />
      </div>}
    </div>
  </section>;
}
