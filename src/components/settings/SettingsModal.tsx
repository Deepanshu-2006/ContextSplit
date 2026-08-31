import React, { useEffect, useState } from "react";
import { X, Shield, LayoutTemplate, Type, Globe, Clock, Info } from "lucide-react";
import { db, PrivacySettings } from "../../services/db";

interface SettingsModalProps {
  onClose: () => void;
}

export function SettingsModal({ onClose }: SettingsModalProps) {
  const [settings, setSettings] = useState<PrivacySettings>({
    enable_active_app: false,
    enable_window_title: false,
    enable_browser_domain: false,
    enable_session_tracking: false,
  });
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    db.getSettings()
      .then((s) => {
        setSettings(s);
        setLoading(false);
      })
      .catch((err) => {
        console.error("Failed to load settings:", err);
        setError("Failed to load settings");
        setLoading(false);
      });
  }, []);

  const handleToggle = async (key: keyof PrivacySettings) => {
    const newSettings = { ...settings, [key]: !settings[key] };
    setSettings(newSettings);
    
    try {
      await db.updateSettings(newSettings);
    } catch (err) {
      console.error("Failed to save settings:", err);
      // Revert on failure
      setSettings(settings);
    }
  };

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/40 backdrop-blur-sm">
      <div 
        className="w-full max-w-lg bg-white rounded-xl shadow-2xl overflow-hidden animate-in fade-in zoom-in-95 duration-200"
        onClick={(e) => e.stopPropagation()}
      >
        <div className="flex items-center justify-between px-6 py-4 border-b border-gray-100">
          <div className="flex items-center gap-2 text-gray-900">
            <Shield size={20} className="text-indigo-500" />
            <h2 className="text-lg font-semibold tracking-tight">Privacy & Context Settings</h2>
          </div>
          <button 
            onClick={onClose}
            className="p-1.5 text-gray-400 hover:text-gray-900 hover:bg-gray-100 rounded-md transition-colors"
          >
            <X size={18} />
          </button>
        </div>

        <div className="p-6">
          <div className="mb-6 flex items-start gap-3 p-3 bg-blue-50/50 text-blue-800 rounded-lg text-sm border border-blue-100/50">
            <Info size={18} className="shrink-0 mt-0.5 text-blue-500" />
            <p className="leading-relaxed">
              Control what contextual signals are captured when you copy items. 
              <strong> All data is processed locally.</strong> Disabling a category stops future collection, but does not delete existing history.
            </p>
          </div>

          {loading ? (
            <div className="py-12 flex justify-center text-gray-400">Loading settings...</div>
          ) : error ? (
            <div className="py-12 flex justify-center text-red-500">{error}</div>
          ) : (
            <div className="space-y-6">
              
              <ToggleRow 
                icon={<LayoutTemplate size={18} />}
                title="Active Application"
                description="Capture the name of the app you were using when copying."
                checked={settings.enable_active_app}
                onChange={() => handleToggle("enable_active_app")}
              />

              <ToggleRow 
                icon={<Type size={18} />}
                title="Window Title"
                description="Capture the specific window or document title. Requires Active Application to be enabled."
                checked={settings.enable_window_title}
                onChange={() => handleToggle("enable_window_title")}
                disabled={!settings.enable_active_app}
              />

              <ToggleRow 
                icon={<Globe size={18} />}
                title="Browser Domain"
                description="Extract domains and URLs from web content."
                checked={settings.enable_browser_domain}
                onChange={() => handleToggle("enable_browser_domain")}
              />

              <ToggleRow 
                icon={<Clock size={18} />}
                title="Session Tracking"
                description="Group items captured closely in time into sessions."
                checked={settings.enable_session_tracking}
                onChange={() => handleToggle("enable_session_tracking")}
              />

            </div>
          )}
        </div>
        
        <div className="px-6 py-4 bg-gray-50 border-t border-gray-100 flex justify-end">
          <button 
            onClick={onClose}
            className="px-4 py-2 bg-white border border-gray-200 text-gray-700 rounded-lg text-sm font-medium hover:bg-gray-50 transition-colors shadow-sm"
          >
            Close
          </button>
        </div>
      </div>
    </div>
  );
}

function ToggleRow({ 
  icon, 
  title, 
  description, 
  checked, 
  onChange,
  disabled = false
}: { 
  icon: React.ReactNode, 
  title: string, 
  description: string, 
  checked: boolean, 
  onChange: () => void,
  disabled?: boolean
}) {
  return (
    <div className={`flex items-start gap-4 ${disabled ? 'opacity-50' : ''}`}>
      <div className={`p-2 rounded-lg mt-0.5 ${checked ? 'bg-indigo-50 text-indigo-600' : 'bg-gray-100 text-gray-500'}`}>
        {icon}
      </div>
      <div className="flex-1">
        <label className="flex items-center justify-between cursor-pointer group">
          <div>
            <div className="font-medium text-gray-900 text-sm">{title}</div>
            <div className="text-sm text-gray-500 leading-relaxed mt-0.5 pr-8">{description}</div>
          </div>
          <div className="relative">
            <input 
              type="checkbox" 
              className="sr-only" 
              checked={checked} 
              onChange={onChange}
              disabled={disabled}
            />
            <div className={`block w-10 h-6 rounded-full transition-colors ${checked ? 'bg-indigo-500' : 'bg-gray-200'}`}></div>
            <div className={`absolute left-1 top-1 bg-white w-4 h-4 rounded-full transition-transform ${checked ? 'translate-x-4' : ''}`}></div>
          </div>
        </label>
      </div>
    </div>
  );
}
