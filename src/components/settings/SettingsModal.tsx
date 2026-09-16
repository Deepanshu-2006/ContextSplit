import React, { useEffect, useState } from "react";
import { createPortal } from "react-dom";
import { X, Shield, LayoutTemplate, Type, Globe, Clock, Info, PauseCircle, Camera } from "lucide-react";
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
    enable_semantic_processing: false,
    semantic_provider: "local",
    enable_semantic_suggestions: false,
    enable_ai_reasoning: true,
    capture_paused: false,
    enable_context_snapshots: true,
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

  return createPortal(
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/40 backdrop-blur-sm">
      <div 
        className="w-full max-w-lg max-h-[90vh] flex flex-col bg-white rounded-xl shadow-2xl overflow-hidden animate-in fade-in zoom-in-95 duration-200"
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

        <div className="p-6 overflow-y-auto">
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
              
              <div className="p-4 rounded-xl border border-amber-200 bg-amber-50/60 dark:bg-amber-950/20 dark:border-amber-800/40">
                <ToggleRow 
                  icon={<PauseCircle size={18} />}
                  title="Pause Automatic Capture"
                  description="Temporarily pause all automatic clipboard and drag-and-drop capture. No items or context signals will be created while paused."
                  checked={!!settings.capture_paused}
                  onChange={() => handleToggle("capture_paused")}
                  activeBg="bg-amber-500"
                  activeIconBg="bg-amber-100 text-amber-700"
                />
              </div>

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

              <ToggleRow 
                icon={<Camera size={18} />}
                title="Context Snapshots"
                description="Automatically construct lightweight, point-in-time contextual snapshots when working. When disabled, ContextSplit will not generate new snapshots."
                checked={settings.enable_context_snapshots ?? true}
                onChange={() => handleToggle("enable_context_snapshots")}
              />

              <div className="pt-6 mt-6 border-t border-gray-100">
                <div className="flex items-center gap-2 text-gray-900 mb-6">
                  <svg xmlns="http://www.w3.org/2000/svg" width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" className="text-purple-500"><path d="M12 5a3 3 0 1 0-5.997.125 4 4 0 0 0-5.224 4.668ab4.8 4.8 0 0 0 1.554 4.316 4 4 0 0 0 2.87 5.068 3 3 0 1 0 5.485 2.115 4 4 0 0 0 5.224-4.668 4.8 4.8 0 0 0-1.554-4.316 4 4 0 0 0-2.87-5.068A3 3 0 1 0 12 5Z"/><path d="M9.12 11.23a3 3 0 0 0 2.21-1.39"/><path d="M9.47 16a3 3 0 0 0 4.14-1.12"/><path d="M12.92 10.92a3 3 0 0 0 2.45 1.05"/></svg>
                  <h3 className="text-base font-semibold tracking-tight">AI & Semantic Context</h3>
                </div>

                <div className="space-y-6">
                  <ToggleRow 
                    icon={<svg xmlns="http://www.w3.org/2000/svg" width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round"><circle cx="12" cy="12" r="10"/><path d="M12 2a14.5 14.5 0 0 0 0 20 14.5 14.5 0 0 0 0-20"/><path d="M2 12h20"/></svg>}
                    title="Semantic Processing"
                    description="Analyze captured text to understand its meaning. All analysis happens locally on your device."
                    checked={settings.enable_semantic_processing}
                    onChange={() => handleToggle("enable_semantic_processing")}
                  />

                  <div className={`flex items-start gap-4 ${!settings.enable_semantic_processing ? 'opacity-50' : ''}`}>
                    <div className="p-2 rounded-lg mt-0.5 bg-gray-100 text-gray-500">
                      <svg xmlns="http://www.w3.org/2000/svg" width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round"><rect width="18" height="18" x="3" y="3" rx="2" ry="2"/><line x1="3" x2="21" y1="9" y2="9"/><line x1="9" x2="9" y1="21" y2="9"/></svg>
                    </div>
                    <div className="flex-1">
                      <label className="flex flex-col gap-2">
                        <div>
                          <div className="font-medium text-gray-900 text-sm">Semantic Provider</div>
                          <div className="text-sm text-gray-500 leading-relaxed mt-0.5">Select the backend used to generate meaning representations.</div>
                        </div>
                        <select 
                          className="w-full mt-2 bg-gray-50 border border-gray-200 text-gray-900 text-sm rounded-lg focus:ring-purple-500 focus:border-purple-500 block p-2.5"
                          value={settings.semantic_provider}
                          onChange={(e) => {
                            const newSettings = { ...settings, semantic_provider: e.target.value };
                            setSettings(newSettings);
                            db.updateSettings(newSettings).catch(console.error);
                          }}
                          disabled={!settings.enable_semantic_processing}
                        >
                          <option value="local">Local ONNX Model (FastEmbed)</option>
                        </select>
                      </label>
                    </div>
                  </div>

                  <ToggleRow 
                    icon={<svg xmlns="http://www.w3.org/2000/svg" width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round"><path d="m3 21 1.9-5.7a8.5 8.5 0 1 1 3.8 3.8z"/></svg>}
                    title="Semantic Suggestions"
                    description="Use meaning to suggest Workspaces when capturing similar items."
                    checked={settings.enable_semantic_suggestions}
                    onChange={() => handleToggle("enable_semantic_suggestions")}
                    disabled={!settings.enable_semantic_processing}
                  />

                  <ToggleRow 
                    icon={<svg xmlns="http://www.w3.org/2000/svg" width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round"><path d="m12 3-1.912 5.813a2 2 0 0 1-1.275 1.275L3 12l5.813 1.912a2 2 0 0 1 1.275 1.275L12 21l1.912-5.813a2 2 0 0 1 1.275-1.275L21 12l-5.813-1.912a2 2 0 0 1-1.275-1.275L12 3Z"/></svg>}
                    title="Remote AI Context Reasoning"
                    description="Allow ContextSplit to send minimal, privacy-scrubbed EvidencePackages to AI reasoners. When disabled, ContextSplit continues functioning exclusively using deterministic Context Engine logic."
                    checked={settings.enable_ai_reasoning}
                    onChange={() => handleToggle("enable_ai_reasoning")}
                  />

                  <div className="pt-4 mt-2">
                    <div className="flex gap-3">
                      <button 
                        onClick={async () => {
                          if (window.confirm("Are you sure you want to delete all generated semantic data? Your original items will NOT be deleted.")) {
                            try {
                              await db.deleteSemanticData();
                              alert("Semantic data deleted successfully.");
                            } catch (e) {
                              console.error(e);
                              alert("Failed to delete semantic data.");
                            }
                          }
                        }}
                        className="px-4 py-2 text-sm font-medium text-red-600 bg-red-50 border border-red-200 rounded-lg hover:bg-red-100 transition-colors"
                      >
                        Delete Semantic Data
                      </button>
                      <button 
                        onClick={async () => {
                          if (window.confirm("This will delete and regenerate all semantic data in the background. Your items will not be duplicated. Continue?")) {
                            try {
                              await db.rebuildSemanticData();
                              alert("Rebuilding semantic data in the background.");
                            } catch (e) {
                              console.error(e);
                              alert("Failed to start rebuilding process.");
                            }
                          }
                        }}
                        className="px-4 py-2 text-sm font-medium text-amber-700 bg-amber-50 border border-amber-200 rounded-lg hover:bg-amber-100 transition-colors"
                      >
                        Rebuild Semantic Data
                      </button>
                    </div>
                  </div>
                </div>
              </div>

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
    </div>,
    document.body
  );
}

function ToggleRow({ 
  icon, 
  title, 
  description, 
  checked, 
  onChange,
  disabled = false,
  activeBg = "bg-indigo-500",
  activeIconBg = "bg-indigo-50 text-indigo-600",
}: { 
  icon: React.ReactNode, 
  title: string, 
  description: string, 
  checked: boolean, 
  onChange: () => void,
  disabled?: boolean,
  activeBg?: string,
  activeIconBg?: string,
}) {
  return (
    <div className={`flex items-start gap-4 ${disabled ? 'opacity-50' : ''}`}>
      <div className={`p-2 rounded-lg mt-0.5 ${checked ? activeIconBg : 'bg-gray-100 text-gray-500'}`}>
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
            <div className={`block w-10 h-6 rounded-full transition-colors ${checked ? activeBg : 'bg-gray-200'}`}></div>
            <div className={`absolute left-1 top-1 bg-white w-4 h-4 rounded-full transition-transform ${checked ? 'translate-x-4' : ''}`}></div>
          </div>
        </label>
      </div>
    </div>
  );
}
